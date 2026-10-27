//! Windows 输入注入与可取消的绝对截止等待。
use super::REPEAT_INJECT_MARKER;
use crate::event::{EventKind, MouseButton, WheelAxis};
use crate::geometry::{DesktopGeometry, VirtualScreen, to_normalized_absolute};
use crate::scheduler::{PacerOutcome, ReplayDriver, StopSignal};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Instant;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CREATE_WAITABLE_TIMER_MANUAL_RESET, CreateEventW,
    CreateWaitableTimerExW, INFINITE, SetEvent, SetWaitableTimer, TIMER_ALL_ACCESS,
    WaitForMultipleObjects,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, KEYEVENTF_UNICODE,
    MAPVK_VK_TO_VSC_EX, MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK,
    MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, MapVirtualKeyW, SendInput,
    VIRTUAL_KEY,
};

struct OwnedHandle(HANDLE);
unsafe impl Send for OwnedHandle {}
unsafe impl Sync for OwnedHandle {}
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub struct NativeDriver {
    stop_event: Arc<OwnedHandle>,
    timer: OwnedHandle,
    virt: VirtualScreen,
    held_keys: BTreeMap<u32, (u32, bool)>,
    held_buttons: BTreeMap<MouseButton, (i32, i32)>,
    held_unicode: BTreeSet<u16>,
}
impl NativeDriver {
    pub fn new(stop: &Arc<StopSignal>, geometry: &DesktopGeometry) -> Result<Self, String> {
        let virt = geometry.virtual_screen().ok_or("无法获取桌面范围")?;
        unsafe {
            let stop_event = Arc::new(OwnedHandle(
                CreateEventW(None, true, false, None)
                    .map_err(|e| format!("创建停止信号失败：{e}"))?,
            ));
            let timer = CreateWaitableTimerExW(
                None,
                None,
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS.0,
            )
            .or_else(|_| {
                CreateWaitableTimerExW(
                    None,
                    None,
                    CREATE_WAITABLE_TIMER_MANUAL_RESET,
                    TIMER_ALL_ACCESS.0,
                )
            })
            .map(OwnedHandle)
            .map_err(|e| format!("创建回放计时器失败：{e}"))?;
            let wake_handle = stop_event.clone();
            stop.set_waker(Arc::new(move || {
                let _ = SetEvent(wake_handle.0);
            }));
            Ok(Self {
                stop_event,
                timer,
                virt,
                held_keys: BTreeMap::new(),
                held_buttons: BTreeMap::new(),
                held_unicode: BTreeSet::new(),
            })
        }
    }
    fn track_held(&mut self, kind: &EventKind) {
        if let Some(position) = kind.position() {
            for p in self.held_buttons.values_mut() {
                *p = position;
            }
        }
        match kind {
            EventKind::KeyDown { vk, scan, extended } => {
                self.held_keys.insert(*vk, (*scan, *extended));
            }
            EventKind::KeyUp { vk, .. } => {
                self.held_keys.remove(vk);
            }
            EventKind::MouseDown { button, x, y } => {
                self.held_buttons.insert(*button, (*x, *y));
            }
            EventKind::MouseMove { x, y } => {
                for p in self.held_buttons.values_mut() {
                    *p = (*x, *y);
                }
            }
            EventKind::MouseUp { button, .. } => {
                self.held_buttons.remove(button);
            }
            EventKind::Unicode { unit, up: false } => {
                self.held_unicode.insert(*unit);
            }
            EventKind::Unicode { unit, up: true } => {
                self.held_unicode.remove(unit);
            }
            _ => {}
        }
    }
}
impl ReplayDriver for NativeDriver {
    fn now(&mut self) -> Instant {
        Instant::now()
    }
    fn wait_until(&mut self, deadline: Instant) -> PacerOutcome {
        loop {
            let now = Instant::now();
            if now >= deadline {
                return PacerOutcome::Deadline;
            }
            let due = -((deadline - now)
                .as_nanos()
                .div_ceil(100)
                .min(i64::MAX as u128) as i64)
                .max(1);
            unsafe {
                if let Err(e) = SetWaitableTimer(self.timer.0, &due, 0, None, None, false) {
                    return PacerOutcome::Failed(format!("回放计时器失败：{e}"));
                }
                let woke =
                    WaitForMultipleObjects(&[self.stop_event.0, self.timer.0], false, INFINITE);
                if woke == WAIT_OBJECT_0 {
                    return PacerOutcome::Cancelled;
                }
                if woke.0 != WAIT_OBJECT_0.0 + 1 {
                    return PacerOutcome::Failed(format!(
                        "等待回放计时器失败：{}",
                        windows::core::Error::from_win32()
                    ));
                }
            }
        }
    }
    fn inject(&mut self, kind: &EventKind) -> Result<(), String> {
        let input = build_input(kind, &self.virt);
        let sent = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
        if sent != 1 {
            return Err(format!(
                "SendInput 返回 {sent}/1，输入未成功发送。请检查目标应用权限是否高于 Repeat。"
            ));
        }
        self.track_held(kind);
        Ok(())
    }
    fn release_held(&mut self) -> Result<Vec<String>, String> {
        let mut released = Vec::new();
        let keys = self.held_keys.clone();
        let buttons = self.held_buttons.clone();
        let unicode = self.held_unicode.clone();
        let mut failed = 0;
        for (vk, (scan, extended)) in keys {
            if self
                .inject(&EventKind::KeyUp { vk, scan, extended })
                .is_ok()
            {
                released.push(crate::event::vk_name(vk));
            } else {
                failed += 1;
            }
        }
        for (button, _) in buttons {
            // 收尾只抬起按钮，不能把用户已经移开的指针拉回最后一次注入位置。
            let (flags, data) = mouse_flags(button, false);
            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dwFlags: MOUSE_EVENT_FLAGS(flags),
                        mouseData: data,
                        dwExtraInfo: REPEAT_INJECT_MARKER,
                        ..Default::default()
                    },
                },
            };
            if unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) } == 1 {
                self.held_buttons.remove(&button);
                released.push(button.display().into());
            } else {
                failed += 1;
            }
        }
        for unit in unicode {
            if self.inject(&EventKind::Unicode { unit, up: true }).is_ok() {
                released.push("文本输入".into());
            } else {
                failed += 1;
            }
        }
        if failed > 0 {
            Err(format!(
                "{failed} 个输入未能释放，请检查目标应用权限并手动松开相应按键"
            ))
        } else {
            Ok(released)
        }
    }
}

fn build_input(kind: &EventKind, virt: &VirtualScreen) -> INPUT {
    match kind {
        EventKind::KeyDown { vk, scan, extended } | EventKind::KeyUp { vk, scan, extended } => {
            let up = matches!(kind, EventKind::KeyUp { .. });
            // 有录制 VK 时用 VK 注入：低级钩子看到的 vkCode 与录制事件一致（捕获与
            // 核对都以钩子层为基准）。扫描码注入会把钩子层 vkCode 翻译成通用修饰键
            // （0x11/0x10），与录制不再一致；仅在事件缺失 VK 时才退回扫描码注入。
            let use_scan = *vk == 0 && *scan != 0;
            // 注入不会自动合成扫描码：事件缺扫描码（编辑重建的按键）时按当前布局从
            // VK 合成，保证消息层 lParam 仍带物理键身份；扩展键盘键取基础扫描码，
            // 扩展位由 KEYEVENTF_EXTENDEDKEY 表达。
            let w_scan = if *scan != 0 || *vk == 0 {
                *scan as u16
            } else {
                unsafe { (MapVirtualKeyW(*vk, MAPVK_VK_TO_VSC_EX) & 0xFF) as u16 }
            };
            let flags = (if use_scan { KEYEVENTF_SCANCODE.0 } else { 0 })
                | (if *extended {
                    KEYEVENTF_EXTENDEDKEY.0
                } else {
                    0
                })
                | (if up { KEYEVENTF_KEYUP.0 } else { 0 });
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(if use_scan { 0 } else { *vk as u16 }),
                        wScan: if use_scan { *scan as u16 } else { w_scan },
                        dwFlags: KEYBD_EVENT_FLAGS(flags),
                        time: 0,
                        dwExtraInfo: REPEAT_INJECT_MARKER,
                    },
                },
            }
        }
        EventKind::Unicode { unit, up } => INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: *unit,
                    dwFlags: KEYBD_EVENT_FLAGS(
                        KEYEVENTF_UNICODE.0 | if *up { KEYEVENTF_KEYUP.0 } else { 0 },
                    ),
                    time: 0,
                    dwExtraInfo: REPEAT_INJECT_MARKER,
                },
            },
        },
        _ => {
            let (x, y) = kind.position().expect("鼠标事件必须带桌面坐标");
            let (nx, ny) = to_normalized_absolute(x, y, virt);
            let (flags, data) = match kind {
                EventKind::MouseMove { .. } => (0, 0),
                EventKind::MouseDown { button, .. } => mouse_flags(*button, true),
                EventKind::MouseUp { button, .. } => mouse_flags(*button, false),
                EventKind::Wheel { axis, delta, .. } => (
                    if *axis == WheelAxis::Vertical {
                        MOUSEEVENTF_WHEEL.0
                    } else {
                        MOUSEEVENTF_HWHEEL.0
                    },
                    *delta as u32,
                ),
                _ => unreachable!(),
            };
            INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: nx,
                        dy: ny,
                        mouseData: data,
                        dwFlags: MOUSE_EVENT_FLAGS(
                            flags
                                | MOUSEEVENTF_MOVE.0
                                | MOUSEEVENTF_ABSOLUTE.0
                                | MOUSEEVENTF_VIRTUALDESK.0,
                        ),
                        time: 0,
                        dwExtraInfo: REPEAT_INJECT_MARKER,
                    },
                },
            }
        }
    }
}
fn mouse_flags(button: MouseButton, down: bool) -> (u32, u32) {
    match (button, down) {
        (MouseButton::Left, true) => (MOUSEEVENTF_LEFTDOWN.0, 0),
        (MouseButton::Left, false) => (MOUSEEVENTF_LEFTUP.0, 0),
        (MouseButton::Right, true) => (MOUSEEVENTF_RIGHTDOWN.0, 0),
        (MouseButton::Right, false) => (MOUSEEVENTF_RIGHTUP.0, 0),
        (MouseButton::Middle, true) => (MOUSEEVENTF_MIDDLEDOWN.0, 0),
        (MouseButton::Middle, false) => (MOUSEEVENTF_MIDDLEUP.0, 0),
        (MouseButton::X1, true) => (MOUSEEVENTF_XDOWN.0, 1),
        (MouseButton::X1, false) => (MOUSEEVENTF_XUP.0, 1),
        (MouseButton::X2, true) => (MOUSEEVENTF_XDOWN.0, 2),
        (MouseButton::X2, false) => (MOUSEEVENTF_XUP.0, 2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn virt() -> VirtualScreen {
        VirtualScreen {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        }
    }

    /// 真实硬件录制同时带 VK 与扫描码：必须走 VK 注入，让低级钩子看到的 vkCode
    /// 与录制事件一致（扫描码路径会把修饰键翻译成通用 VK，钩子层不再忠实）。
    /// 仅 VK 缺失时才用扫描码注入；事件缺扫描码时（编辑重建）按布局从 VK 合成。
    #[test]
    fn recorded_keys_with_vk_inject_by_vk_even_when_scan_present() {
        let vk_of = |input: &INPUT| unsafe { input.Anonymous.ki.wVk.0 };
        let flags_of = |input: &INPUT| unsafe { input.Anonymous.ki.dwFlags.0 };
        let scan_of = |input: &INPUT| unsafe { input.Anonymous.ki.wScan };
        let down = build_input(
            &EventKind::KeyDown {
                vk: 0xA2,
                scan: 0x1D,
                extended: false,
            },
            &virt(),
        );
        assert_eq!(vk_of(&down), 0xA2);
        assert_eq!(scan_of(&down), 0x1D);
        assert_eq!(
            flags_of(&down) & KEYEVENTF_SCANCODE.0,
            0,
            "有 VK 时不应使用扫描码注入"
        );
        let right = build_input(
            &EventKind::KeyDown {
                vk: 0xA3,
                scan: 0x1D,
                extended: true,
            },
            &virt(),
        );
        assert_eq!(vk_of(&right), 0xA3);
        assert_ne!(flags_of(&right) & KEYEVENTF_EXTENDEDKEY.0, 0);
        // 编辑重建的按键没有原始扫描码：按当前布局从 VK 合成，消息层保留物理键身份。
        let synthesized = build_input(
            &EventKind::KeyDown {
                vk: 0x6B,
                scan: 0,
                extended: false,
            },
            &virt(),
        );
        assert_eq!(vk_of(&synthesized), 0x6B);
        assert_eq!(scan_of(&synthesized), 0x4E, "小键盘+ 应合成扫描码 0x4E");
        let synthesized_right = build_input(
            &EventKind::KeyDown {
                vk: 0xA3,
                scan: 0,
                extended: true,
            },
            &virt(),
        );
        assert_eq!(scan_of(&synthesized_right), 0x1D, "右Ctrl 取基础扫描码");
        assert_ne!(
            flags_of(&synthesized_right) & KEYEVENTF_EXTENDEDKEY.0,
            0,
            "扩展位仍由标志表达"
        );
        let scan_only = build_input(
            &EventKind::KeyDown {
                vk: 0,
                scan: 0x1E,
                extended: false,
            },
            &virt(),
        );
        assert_eq!(vk_of(&scan_only), 0);
        assert_ne!(flags_of(&scan_only) & KEYEVENTF_SCANCODE.0, 0);
    }
}
