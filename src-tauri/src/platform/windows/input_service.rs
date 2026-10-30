//! Windows 输入服务线程：
//! - 专用线程安装 `WH_KEYBOARD_LL` / `WH_MOUSE_LL` 并维持消息循环；
//! - 回调只复制必要字段、读时间戳、写入有界缓冲，随即 CallNextHookEx 继续钩子链；
//! - 同一线程注册全局热键，默认 F8 启停录制、F9 停止执行，窗口失焦仍可触发；
//! - 控制命令通过线程消息投递，钩子的安装/卸载都在本线程内完成。
//!
//! 钩子回调不写库、不做 IPC、不打日志（原始键盘数据不进入普通日志）。

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HHOOK, KBDLLHOOKSTRUCT, LLKHF_EXTENDED,
    LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PostThreadMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP, WM_HOTKEY, WM_KEYDOWN, WM_KEYUP,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN,
    WM_XBUTTONUP,
};

use crate::capture::RecordingBuffer;
use crate::event::{EventKind, MouseButton, WheelAxis, wheel_delta_from_mouse_data};

use crate::platform::{HotkeyAction, ServiceCommand};

pub const VK_F8: u16 = 0x77;
pub const VK_F9: u16 = 0x78;
const HOTKEY_ID_TOGGLE_RECORD: i32 = 1;
const HOTKEY_ID_STOP_EXEC: i32 = 2;

/// 线程消息：通知服务线程处理命令队列。
const WM_APP_SERVICE_CMD: u32 = WM_APP + 1;

/// 服务线程的线程本地状态：钩子句柄、当前录制缓冲与热键回调。
/// 低级钩子回调只会被安装它的那个线程（本服务线程）在消息循环中调用。
struct ThreadState {
    keyboard_hook: Option<HHOOK>,
    execution_active: bool,
    stop_vk: u32,
    stop_hotkey_available: bool,
    mouse_hook: Option<HHOOK>,
    swallow_left_up: bool,
    buffer: Option<Arc<Mutex<RecordingBuffer>>>,
    capture_id: u64,
    preparation: Option<crate::platform::RecordingPreparation>,
    on_hotkey: Option<Arc<dyn Fn(HotkeyAction) + Send + Sync>>,
}

thread_local! {
    static STATE: RefCell<ThreadState> = RefCell::new(ThreadState {
        keyboard_hook: None,
        execution_active: false,
        stop_vk: 0x78,
        stop_hotkey_available: false,
        mouse_hook: None,
        swallow_left_up: false,
        buffer: None,
        capture_id: 0,
        preparation: None,
        on_hotkey: None,
    });
}

pub struct InputServiceHandle {
    thread_id: u32,
    hotkeys_available: Arc<HotkeyAvailability>,
    queue: Arc<Mutex<VecDeque<ServiceCommand>>>,
    status: Arc<Mutex<Vec<String>>>,
    join: Mutex<Option<std::thread::JoinHandle<()>>>,
}

#[derive(Default)]
struct HotkeyAvailability {
    record: AtomicBool,
    stop: AtomicBool,
}

impl HotkeyAvailability {
    fn update(&self, record: bool, stop: bool) {
        self.record.store(record, Ordering::SeqCst);
        self.stop.store(stop, Ordering::SeqCst);
    }

    fn registered_vks(&self, hotkeys: &crate::capture::Hotkeys) -> Vec<u32> {
        let mut keys = Vec::with_capacity(2);
        if self.record.load(Ordering::SeqCst) {
            keys.push(hotkeys.record_vk);
        }
        if self.stop.load(Ordering::SeqCst) {
            keys.push(hotkeys.stop_vk);
        }
        keys
    }
}

impl InputServiceHandle {
    /// 投递控制命令（非阻塞）。命令在服务线程的消息循环里执行。
    pub fn send_cmd(&self, cmd: ServiceCommand) -> Result<(), String> {
        let mut queue = self.queue.lock().unwrap();
        queue.push_back(cmd);
        unsafe {
            if let Err(error) =
                PostThreadMessageW(self.thread_id, WM_APP_SERVICE_CMD, WPARAM(0), LPARAM(0))
            {
                queue.pop_back();
                return Err(format!("输入服务没有响应：{error}"));
            }
        }
        Ok(())
    }

    pub fn start_capture(&self, buffer: Arc<Mutex<RecordingBuffer>>) -> Result<u64, String> {
        let (tx, rx) = mpsc::channel();
        self.send_cmd(ServiceCommand::StartCaptureChecked(buffer, tx))?;
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "安装输入钩子超时")?
    }
    pub fn stop_capture(&self) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.send_cmd(ServiceCommand::StopCaptureChecked(tx))?;
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "停止输入钩子超时")?
    }
    pub fn set_hotkeys(&self, hotkeys: crate::capture::Hotkeys) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.send_cmd(ServiceCommand::SetHotkeys(hotkeys, tx))?;
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "更新快捷键超时")?
    }
    pub fn initialize_hotkeys(&self, hotkeys: crate::capture::Hotkeys) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.send_cmd(ServiceCommand::InitializeHotkeys(hotkeys, tx))?;
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "初始化快捷键超时")?
    }
    pub fn record_hotkey_available(&self) -> bool {
        self.hotkeys_available.record.load(Ordering::SeqCst)
    }
    pub fn stop_hotkey_available(&self) -> bool {
        self.hotkeys_available.stop.load(Ordering::SeqCst)
    }
    pub fn registered_hotkey_vks(&self, hotkeys: &crate::capture::Hotkeys) -> Vec<u32> {
        self.hotkeys_available.registered_vks(hotkeys)
    }

    pub fn arm_execution(&self, active: bool) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.send_cmd(ServiceCommand::ArmExecution(active, tx))?;
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "启用原生停止监听超时")?
    }
    pub fn prepare_recording(
        &self,
        preparation: Option<crate::platform::RecordingPreparation>,
    ) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.send_cmd(ServiceCommand::PrepareRecording(preparation, tx))?;
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "启用录制准备监听超时")?
    }
    pub fn status_lines(&self) -> Vec<String> {
        self.status.lock().unwrap().clone()
    }

    /// 通知服务线程退出并等待其结束（应用退出时调用）。
    pub fn shutdown(&self) {
        let join = self.join.lock().unwrap().take();
        if let Some(join) = join {
            let _ = self.send_cmd(ServiceCommand::Shutdown);
            let _ = join.join();
        }
    }
}

impl Drop for InputServiceHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// 启动输入服务线程并注册全局热键。热键注册结果通过 status 返回（冲突不阻塞启动）。
pub fn start_input_service(
    on_hotkey: Arc<dyn Fn(HotkeyAction) + Send + Sync>,
) -> Result<InputServiceHandle, String> {
    let queue = Arc::new(Mutex::new(VecDeque::<ServiceCommand>::new()));
    let status = Arc::new(Mutex::new(Vec::<String>::new()));
    let hotkeys_available = Arc::new(HotkeyAvailability::default());
    let ready_for_thread = hotkeys_available.clone();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();

    let queue_for_thread = queue.clone();
    let status_for_thread = status.clone();
    let join = std::thread::Builder::new()
        .name("repeat-input-service".into())
        .spawn(move || {
            service_thread_main(
                on_hotkey,
                queue_for_thread,
                status_for_thread,
                ready_tx,
                ready_for_thread,
            );
        })
        .map_err(|e| format!("启动输入服务线程失败：{e}"))?;

    // 服务线程注册完热键后回报线程 ID；此后 status 里即有热键注册结果。
    let thread_id = ready_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| "输入服务线程未在 5 秒内就绪".to_string())??;

    Ok(InputServiceHandle {
        thread_id,
        hotkeys_available,
        queue,
        status,
        join: Mutex::new(Some(join)),
    })
}

fn service_thread_main(
    on_hotkey: Arc<dyn Fn(HotkeyAction) + Send + Sync>,
    queue: Arc<Mutex<VecDeque<ServiceCommand>>>,
    status: Arc<Mutex<Vec<String>>>,
    ready_tx: mpsc::Sender<Result<u32, String>>,
    hotkeys_available: Arc<HotkeyAvailability>,
) {
    let _environment = match super::environment::EnvironmentWatch::new(on_hotkey.clone()) {
        Ok(watch) => watch,
        Err(reason) => {
            let _ = ready_tx.send(Err(reason));
            return;
        }
    };
    STATE.with(|s| {
        s.borrow_mut().on_hotkey = Some(on_hotkey);
    });

    if let Err(error) = install_keyboard_hook() {
        let _ = ready_tx.send(Err(format!("无法安装停止监听：{error}")));
        return;
    }
    unsafe {
        let tid = GetCurrentThreadId();
        // 注册全局热键；冲突时记录原因并继续运行（界面按钮始终是主入口）。
        let mut hotkeys = crate::capture::Hotkeys::default();
        register_hotkeys_independently(&hotkeys, &hotkeys_available, &status);
        sync_stop_listener(&hotkeys, &hotkeys_available);

        let _ = ready_tx.send(Ok(tid));

        let mut msg = MSG::default();
        let mut expected_shutdown = false;
        'main: loop {
            let r = GetMessageW(&mut msg, None, 0, 0);
            if r.0 == 0 || r.0 == -1 {
                // WM_QUIT 或消息循环错误
                break 'main;
            }
            match msg.message {
                // 排空命令队列；队列空（None）只结束本轮排空，服务线程继续消息循环。
                // 此前误把 None 也 break 到外层循环，导致装完钩子线程立即退出。
                WM_APP_SERVICE_CMD => loop {
                    let cmd = { queue.lock().unwrap().pop_front() };
                    let Some(cmd) = cmd else {
                        break;
                    };
                    match cmd {
                        ServiceCommand::StartCapture(buffer) => match renew_hooks() {
                            Ok(()) => {
                                STATE.with(|s| s.borrow_mut().buffer = Some(buffer));
                                status.lock().unwrap().push("系统钩子已安装".into());
                            }
                            Err(e) => {
                                status
                                    .lock()
                                    .unwrap()
                                    .push(format!("安装系统钩子失败：{e}"));
                            }
                        },
                        ServiceCommand::StopCapture => {
                            stop_capture_hooks();
                        }
                        ServiceCommand::StartCaptureChecked(buffer, reply) => {
                            let result = renew_hooks()
                                .map_err(|e| format!("安装输入钩子失败：{e}"))
                                .and_then(|()| {
                                    STATE.with(|s| {
                                        let mut st = s.borrow_mut();
                                        if st.preparation.as_ref().is_some_and(|p| p.is_cancelled())
                                        {
                                            return Err("录制准备已取消".into());
                                        }
                                        if st.preparation.is_some() {
                                            // A down during preparation must not become an
                                            // unmatched up in the new recording.
                                            super::recording_ready()?;
                                        }
                                        st.preparation = None;
                                        st.capture_id = st.capture_id.wrapping_add(1);
                                        st.buffer = Some(buffer);
                                        Ok(st.capture_id)
                                    })
                                });
                            let _ = reply.send(result);
                        }
                        ServiceCommand::StopCaptureChecked(reply) => {
                            stop_capture_hooks();
                            let _ = reply.send(Ok(()));
                        }
                        ServiceCommand::InitializeHotkeys(next, reply) => {
                            let result = next.validate().map(|()| {
                                unregister_hotkeys();
                                register_hotkeys_independently(&next, &hotkeys_available, &status);
                                hotkeys = next;
                                sync_stop_listener(&hotkeys, &hotkeys_available);
                            });
                            let _ = reply.send(result);
                        }
                        ServiceCommand::SetHotkeys(next, reply) => {
                            let result =
                                configure_hotkeys(&hotkeys, &next, &hotkeys_available, &status);
                            if result.is_ok() {
                                hotkeys = next;
                            }
                            sync_stop_listener(&hotkeys, &hotkeys_available);
                            let _ = reply.send(result);
                        }
                        ServiceCommand::ArmExecution(active, reply) => {
                            let result = if active {
                                renew_hooks().map_err(|e| format!("无法监听执行停止控件：{e}"))
                            } else {
                                Ok(())
                            };
                            if result.is_ok() {
                                STATE.with(|s| s.borrow_mut().execution_active = active);
                            }
                            let _ = reply.send(result);
                        }
                        ServiceCommand::PrepareRecording(preparation, reply) => {
                            let result = if preparation.is_some() {
                                renew_hooks().map_err(|e| format!("无法监听录制准备取消控件：{e}"))
                            } else {
                                Ok(())
                            };
                            if result.is_ok() {
                                STATE.with(|s| s.borrow_mut().preparation = preparation);
                            }
                            let _ = reply.send(result);
                        }
                        #[cfg(test)]
                        ServiceCommand::InvalidateCaptureHooks(reply) => {
                            // Simulate Windows silently removing a timed-out
                            // hook while the cached HHOOK still remains set.
                            STATE.with(|s| {
                                let st = s.borrow();
                                for hook in [st.keyboard_hook, st.mouse_hook].into_iter().flatten()
                                {
                                    let _ = UnhookWindowsHookEx(hook);
                                }
                            });
                            let _ = reply.send(());
                        }
                        ServiceCommand::Shutdown => {
                            expected_shutdown = true;
                            break 'main;
                        }
                    }
                },
                WM_HOTKEY => {
                    let action = match msg.wParam.0 as i32 {
                        HOTKEY_ID_TOGGLE_RECORD => Some(STATE.with(|s| {
                            s.borrow()
                                .preparation
                                .as_ref()
                                .map_or(HotkeyAction::ToggleRecording, |p| p.cancel())
                        })),
                        // WM_HOTKEY does not expose injection flags; the low-level hook
                        // handles the physical stop key instead.
                        HOTKEY_ID_STOP_EXEC => None,
                        _ => None,
                    };
                    if let Some(action) = action {
                        let cb = STATE.with(|s| s.borrow().on_hotkey.clone());
                        if let Some(cb) = cb {
                            cb(action);
                        }
                    }
                }
                _ => {
                    DispatchMessageW(&msg);
                }
            }
        }

        hotkeys_available.update(false, false);
        if !expected_shutdown && let Some(callback) = STATE.with(|s| s.borrow().on_hotkey.clone()) {
            callback(HotkeyAction::EnvironmentChanged);
        }
        uninstall_hooks();
        let _ = UnregisterHotKey(None, HOTKEY_ID_TOGGLE_RECORD);
        let _ = UnregisterHotKey(None, HOTKEY_ID_STOP_EXEC);
    }
}

/// 诊断计数：钩子回调被系统调用的次数 / 其中拿到录制缓冲的次数（原生验证用）。
static HOOK_CALLBACKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static HOOK_WITH_BUFFER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn hook_diagnostics() -> (u64, u64) {
    (
        HOOK_CALLBACKS.load(std::sync::atomic::Ordering::Relaxed),
        HOOK_WITH_BUFFER.load(std::sync::atomic::Ordering::Relaxed),
    )
}

fn install_keyboard_hook() -> windows::core::Result<()> {
    if STATE.with(|s| s.borrow().keyboard_hook.is_some()) {
        return Ok(());
    }
    unsafe {
        let module = GetModuleHandleW(None)?;
        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(low_level_keyboard_proc),
            Some(HINSTANCE(module.0)),
            0,
        )?;
        STATE.with(|s| s.borrow_mut().keyboard_hook = Some(hook));
    }
    Ok(())
}
fn renew_hooks() -> windows::core::Result<()> {
    // Windows can silently remove a timed-out low-level hook. A cached HHOOK
    // cannot prove it is still listening, so each new operation installs a fresh
    // pair. Keep the old pair until both replacements succeed and retain the
    // state that pairs a swallowed overlay click with its eventual button up.
    unsafe {
        let module = GetModuleHandleW(None)?;
        let keyboard = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(low_level_keyboard_proc),
            Some(HINSTANCE(module.0)),
            0,
        )?;
        let mouse = match SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(low_level_mouse_proc),
            Some(HINSTANCE(module.0)),
            0,
        ) {
            Ok(hook) => hook,
            Err(error) => {
                let _ = UnhookWindowsHookEx(keyboard);
                return Err(error);
            }
        };
        let previous = STATE.with(|s| {
            let mut state = s.borrow_mut();
            (
                state.keyboard_hook.replace(keyboard),
                state.mouse_hook.replace(mouse),
            )
        });
        for hook in [previous.0, previous.1].into_iter().flatten() {
            let _ = UnhookWindowsHookEx(hook);
        }
    }
    Ok(())
}
fn stop_capture_hooks() {
    // Keep the mouse hook installed so a swallowed physical down is always
    // paired with a swallowed up, even if execution finishes in between.
    STATE.with(|s| s.borrow_mut().buffer = None);
}

fn uninstall_hooks() {
    unsafe {
        let (kb, mouse) = STATE.with(|s| {
            let mut st = s.borrow_mut();
            (st.keyboard_hook.take(), st.mouse_hook.take())
        });
        if let Some(h) = kb {
            let _ = UnhookWindowsHookEx(h);
        }
        if let Some(h) = mouse {
            let _ = UnhookWindowsHookEx(h);
        }
        STATE.with(|s| s.borrow_mut().buffer = None);
    }
}

/// 键盘消息 → 事件。WM_KEYDOWN 与 WM_SYSKEYDOWN 在低级钩子层等价处理。
pub(crate) fn keyboard_event_kind(msg: u32, info: &KBDLLHOOKSTRUCT) -> Option<EventKind> {
    let vk = info.vkCode;
    let scan = info.scanCode;
    let extended = (info.flags & LLKHF_EXTENDED).0 != 0;
    match msg {
        WM_KEYDOWN | WM_SYSKEYDOWN => Some(EventKind::KeyDown { vk, scan, extended }),
        WM_KEYUP | WM_SYSKEYUP => Some(EventKind::KeyUp { vk, scan, extended }),
        _ => None,
    }
}

/// 鼠标消息 → 事件。坐标为虚拟桌面物理像素（进程声明 PMv2 后不再被虚拟化）。
pub(crate) fn mouse_event_kind(msg: u32, info: &MSLLHOOKSTRUCT) -> Option<EventKind> {
    let pt = info.pt;
    match msg {
        WM_MOUSEMOVE => Some(EventKind::MouseMove { x: pt.x, y: pt.y }),
        WM_LBUTTONDOWN => Some(EventKind::MouseDown {
            button: MouseButton::Left,
            x: pt.x,
            y: pt.y,
        }),
        WM_LBUTTONUP => Some(EventKind::MouseUp {
            button: MouseButton::Left,
            x: pt.x,
            y: pt.y,
        }),
        WM_RBUTTONDOWN => Some(EventKind::MouseDown {
            button: MouseButton::Right,
            x: pt.x,
            y: pt.y,
        }),
        WM_RBUTTONUP => Some(EventKind::MouseUp {
            button: MouseButton::Right,
            x: pt.x,
            y: pt.y,
        }),
        WM_MBUTTONDOWN => Some(EventKind::MouseDown {
            button: MouseButton::Middle,
            x: pt.x,
            y: pt.y,
        }),
        WM_MBUTTONUP => Some(EventKind::MouseUp {
            button: MouseButton::Middle,
            x: pt.x,
            y: pt.y,
        }),
        WM_XBUTTONDOWN | WM_XBUTTONUP => {
            let xb = (info.mouseData >> 16) & 0xFFFF;
            let button = match xb {
                1 => MouseButton::X1,
                2 => MouseButton::X2,
                _ => return None,
            };
            if msg == WM_XBUTTONDOWN {
                Some(EventKind::MouseDown {
                    button,
                    x: pt.x,
                    y: pt.y,
                })
            } else {
                Some(EventKind::MouseUp {
                    button,
                    x: pt.x,
                    y: pt.y,
                })
            }
        }
        WM_MOUSEWHEEL => Some(EventKind::Wheel {
            axis: WheelAxis::Vertical,
            delta: wheel_delta_from_mouse_data(info.mouseData),
            x: pt.x,
            y: pt.y,
        }),
        WM_MOUSEHWHEEL => Some(EventKind::Wheel {
            axis: WheelAxis::Horizontal,
            delta: wheel_delta_from_mouse_data(info.mouseData),
            x: pt.x,
            y: pt.y,
        }),
        _ => None,
    }
}

/// 本工具注入的输入（带标记）不录回自己；第三方注入仍按系统输入忠实记录。
pub(crate) fn is_injected_by_repeat(extra_info: usize) -> bool {
    extra_info == super::REPEAT_INJECT_MARKER
}

fn is_physical_stop_key(kind: &EventKind, flags: u32, stop_vk: u32, stop_available: bool) -> bool {
    flags & LLKHF_INJECTED.0 == 0
        && matches!(kind, EventKind::KeyDown { vk, .. } if *vk == 0x1B || (stop_available && *vk == stop_vk))
}

fn is_physical_left_down(
    msg: u32,
    flags: u32,
    x: i32,
    y: i32,
    region: Option<crate::platform::StopRegion>,
) -> bool {
    msg == WM_LBUTTONDOWN
        && flags & LLMHF_INJECTED == 0
        && region.is_some_and(|region| region.contains(x, y))
}

fn classify_stop_mouse_event(
    pending_up: &mut bool,
    active: bool,
    msg: u32,
    flags: u32,
    x: i32,
    y: i32,
    region: Option<crate::platform::StopRegion>,
) -> (bool, bool) {
    if msg == WM_LBUTTONUP && flags & LLMHF_INJECTED == 0 && *pending_up {
        *pending_up = false;
        return (true, false);
    }
    if active && is_physical_left_down(msg, flags, x, y, region) {
        *pending_up = true;
        return (true, true);
    }
    (false, false)
}

fn overlay_stop_action(
    execution_active: bool,
    capture_id: u64,
    preparation: Option<&crate::platform::RecordingPreparation>,
) -> HotkeyAction {
    if let Some(preparation) = preparation {
        preparation.cancel()
    } else if execution_active {
        HotkeyAction::StopExecution
    } else {
        HotkeyAction::StopRecording(capture_id)
    }
}

unsafe extern "system" fn low_level_keyboard_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if code < 0 {
            return CallNextHookEx(None, code, wparam, lparam);
        }
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        HOOK_CALLBACKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Some(kind) = keyboard_event_kind(wparam.0 as u32, info)
            && !is_injected_by_repeat(info.dwExtraInfo)
        {
            let (buffer, callback) = STATE.with(|s| {
                let st = s.borrow();
                let stop = (st.execution_active || st.preparation.is_some())
                    && is_physical_stop_key(
                        &kind,
                        info.flags.0,
                        st.stop_vk,
                        st.stop_hotkey_available,
                    );
                (
                    st.buffer.clone(),
                    if stop {
                        st.on_hotkey.clone().map(|cb| {
                            (
                                cb,
                                overlay_stop_action(
                                    st.execution_active,
                                    st.capture_id,
                                    st.preparation.as_ref(),
                                ),
                            )
                        })
                    } else {
                        None
                    },
                )
            });
            if let Some((callback, action)) = callback {
                callback(action);
            }
            if let Some(buffer) = buffer {
                HOOK_WITH_BUFFER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                buffer.lock().unwrap().push(Instant::now(), info.time, kind);
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }
}

unsafe extern "system" fn low_level_mouse_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if code < 0 {
            return CallNextHookEx(None, code, wparam, lparam);
        }
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        HOOK_CALLBACKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let msg = wparam.0 as u32;
        let (swallow, callback) = STATE.with(|s| {
            let mut st = s.borrow_mut();
            let active = st.execution_active || st.buffer.is_some() || st.preparation.is_some();
            let region = if active && msg == WM_LBUTTONDOWN && info.flags & LLMHF_INJECTED == 0 {
                *crate::platform::OVERLAY_STOP_REGION.lock().unwrap()
            } else {
                None
            };
            let (swallow, stop) = classify_stop_mouse_event(
                &mut st.swallow_left_up,
                active,
                msg,
                info.flags,
                info.pt.x,
                info.pt.y,
                region,
            );
            (
                swallow,
                if stop {
                    st.on_hotkey.clone().map(|cb| {
                        (
                            cb,
                            overlay_stop_action(
                                st.execution_active,
                                st.capture_id,
                                st.preparation.as_ref(),
                            ),
                        )
                    })
                } else {
                    None
                },
            )
        });
        if let Some((callback, action)) = callback {
            callback(action);
        }
        if swallow {
            return LRESULT(1);
        }

        if let Some(kind) = mouse_event_kind(wparam.0 as u32, info)
            && !is_injected_by_repeat(info.dwExtraInfo)
            && let Some(buffer) = STATE.with(|s| s.borrow().buffer.clone())
        {
            HOOK_WITH_BUFFER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let at = Instant::now();
            let mut b = buffer.lock().unwrap();
            b.push(at, info.time, kind);
        }
        CallNextHookEx(None, code, wparam, lparam)
    }
}

fn sync_stop_listener(hotkeys: &crate::capture::Hotkeys, available: &HotkeyAvailability) {
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        st.stop_vk = hotkeys.stop_vk;
        st.stop_hotkey_available = available.stop.load(Ordering::SeqCst);
    });
}

fn hotkey_status_lines(hotkeys: &crate::capture::Hotkeys, record: bool, stop: bool) -> Vec<String> {
    let state = |available| {
        if available {
            "已注册"
        } else {
            "注册失败，可能被其他程序占用"
        }
    };
    vec![
        format!(
            "{} 启停录制：{}",
            crate::event::vk_name(hotkeys.record_vk),
            state(record)
        ),
        format!(
            "{} 停止执行：{}",
            crate::event::vk_name(hotkeys.stop_vk),
            state(stop)
        ),
    ]
}

fn unregister_hotkeys() {
    unsafe {
        let _ = UnregisterHotKey(None, HOTKEY_ID_TOGGLE_RECORD);
        let _ = UnregisterHotKey(None, HOTKEY_ID_STOP_EXEC);
    }
}

fn register_hotkeys_independently(
    hotkeys: &crate::capture::Hotkeys,
    available: &HotkeyAvailability,
    status: &Mutex<Vec<String>>,
) {
    let (record, stop) = unsafe {
        (
            RegisterHotKey(
                None,
                HOTKEY_ID_TOGGLE_RECORD,
                MOD_NOREPEAT,
                hotkeys.record_vk,
            )
            .is_ok(),
            RegisterHotKey(None, HOTKEY_ID_STOP_EXEC, MOD_NOREPEAT, hotkeys.stop_vk).is_ok(),
        )
    };
    available.update(record, stop);
    *status.lock().unwrap() = hotkey_status_lines(hotkeys, record, stop);
}

fn configure_hotkeys(
    old: &crate::capture::Hotkeys,
    next: &crate::capture::Hotkeys,
    available: &HotkeyAvailability,
    status: &Mutex<Vec<String>>,
) -> Result<(), String> {
    next.validate()?;
    available.update(false, false);
    unregister_hotkeys();
    unsafe {
        let result = RegisterHotKey(None, HOTKEY_ID_TOGGLE_RECORD, MOD_NOREPEAT, next.record_vk)
            .and_then(|_| RegisterHotKey(None, HOTKEY_ID_STOP_EXEC, MOD_NOREPEAT, next.stop_vk));
        if let Err(error) = result {
            unregister_hotkeys();
            register_hotkeys_independently(old, available, status);
            return Err(format!(
                "快捷键被占用，设置未保存：{error}。请换用其他快捷键"
            ));
        }
    }
    available.update(true, true);
    *status.lock().unwrap() = hotkey_status_lines(next, true, true);
    Ok(())
}

#[cfg(test)]
mod stop_control_tests {
    use super::*;

    #[test]
    #[ignore = "真实 Windows 钩子故障注入与固定 F6/鼠标样本，需交互桌面"]
    fn native_capture_recovers_removed_idle_hooks() {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
            MOUSEEVENTF_MOVE, MOUSEINPUT, SendInput, VIRTUAL_KEY,
        };
        crate::platform::init_process_dpi();
        let service = start_input_service(Arc::new(|_| {})).unwrap();
        service
            .prepare_recording(Some(crate::platform::RecordingPreparation::new(1)))
            .unwrap();
        service.prepare_recording(None).unwrap();
        let (tx, rx) = mpsc::channel();
        service
            .send_cmd(ServiceCommand::InvalidateCaptureHooks(tx))
            .unwrap();
        rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        let buffer = Arc::new(Mutex::new(RecordingBuffer::default()));
        buffer.lock().unwrap().begin();
        service
            .prepare_recording(Some(crate::platform::RecordingPreparation::new(2)))
            .unwrap();
        service.start_capture(buffer.clone()).unwrap();
        let keys = [false, true].map(|up| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0x75),
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        Default::default()
                    },
                    dwExtraInfo: 0x54455354,
                    ..Default::default()
                },
            },
        });
        assert_eq!(unsafe { SendInput(&keys, size_of::<INPUT>() as i32) }, 2);
        for dx in [8, -8] {
            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx,
                        dwFlags: MOUSEEVENTF_MOVE,
                        dwExtraInfo: 0x54455354,
                        ..Default::default()
                    },
                },
            };
            assert_eq!(unsafe { SendInput(&[input], size_of::<INPUT>() as i32) }, 1);
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        service.stop_capture().unwrap();
        let finished = buffer.lock().unwrap().finish();
        service.shutdown();
        let key_events: Vec<_> = finished
            .events
            .iter()
            .filter(|event| event.kind.vk() == Some(0x75))
            .collect();
        let mouse_events = finished
            .events
            .iter()
            .filter(|event| matches!(event.kind, EventKind::MouseMove { .. }))
            .count();
        assert_eq!(key_events.len(), 2, "失效的键盘钩子必须在录制前重新安装");
        assert!(key_events[0].kind.is_key_down() && key_events[1].kind.is_key_up());
        assert!(mouse_events >= 2, "失效的鼠标钩子必须在录制前重新安装");
        let evidence = serde_json::json!({"fault":"OS-removed idle hooks with cached handles","fixedKeyEvents":key_events,"mouseEventCount":mouse_events,"balanced":crate::document::validate_balanced(&finished.events).is_ok()});
        std::fs::write(
            "target/removed-hooks-evidence.json",
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();
        println!("REMOVED_HOOKS_EVIDENCE {evidence}");
    }
    use crate::platform::StopRegion;

    #[test]
    fn conflicting_hotkeys_are_reported_and_not_filtered_from_recording() {
        let hotkeys = crate::capture::Hotkeys::default();
        let availability = HotkeyAvailability::default();
        availability.update(false, true);
        assert_eq!(availability.registered_vks(&hotkeys), vec![hotkeys.stop_vk]);
        let status = hotkey_status_lines(&hotkeys, false, true);
        assert!(status[0].contains("F8") && status[0].contains("注册失败"));
        assert!(status[1].contains("F9") && status[1].contains("已注册"));
        let mut filter = crate::capture::CaptureFilter::new(availability.registered_vks(&hotkeys));
        let key = |vk| EventKind::KeyDown {
            vk,
            scan: 0,
            extended: false,
        };
        assert!(filter.should_record(&key(hotkeys.record_vk)));
        assert!(!filter.should_record(&key(hotkeys.stop_vk)));
        availability.update(true, false);
        assert_eq!(
            availability.registered_vks(&hotkeys),
            vec![hotkeys.record_vk]
        );
    }

    #[test]
    fn recording_overlay_stop_is_bound_to_capture() {
        assert_eq!(
            overlay_stop_action(false, 42, None),
            HotkeyAction::StopRecording(42)
        );
        assert_eq!(
            overlay_stop_action(true, 42, None),
            HotkeyAction::StopExecution
        );
    }

    #[test]
    fn preparation_stop_cancels_only_its_own_token() {
        let first = crate::platform::RecordingPreparation::new(10);
        let second = crate::platform::RecordingPreparation::new(11);
        assert_eq!(
            overlay_stop_action(false, 42, Some(&first)),
            HotkeyAction::CancelRecordingPreparation(10)
        );
        assert!(first.is_cancelled());
        assert!(!second.is_cancelled());
        assert_eq!(
            overlay_stop_action(false, 43, None),
            HotkeyAction::StopRecording(43)
        );
    }

    #[test]
    fn stop_region_only_accepts_physical_left_down_inside_bounds() {
        let region = Some(StopRegion {
            x: -100,
            y: 50,
            width: 40,
            height: 40,
        });
        assert!(is_physical_left_down(WM_LBUTTONDOWN, 0, -100, 50, region));
        assert!(is_physical_left_down(WM_LBUTTONDOWN, 0, -61, 89, region));
        assert!(!is_physical_left_down(WM_LBUTTONDOWN, 0, -60, 89, region));
        assert!(!is_physical_left_down(
            WM_LBUTTONDOWN,
            LLMHF_INJECTED,
            -100,
            50,
            region
        ));
        assert!(!is_physical_left_down(WM_LBUTTONUP, 0, -100, 50, region));
        assert!(!is_physical_left_down(WM_MOUSEMOVE, 0, -100, 50, region));
    }

    #[test]
    fn swallowed_up_survives_execution_end_and_injected_events() {
        let region = Some(StopRegion {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        });
        let mut pending = false;
        assert_eq!(
            classify_stop_mouse_event(
                &mut pending,
                true,
                WM_LBUTTONDOWN,
                LLMHF_INJECTED,
                2,
                2,
                region
            ),
            (false, false)
        );
        assert!(!pending);
        assert_eq!(
            classify_stop_mouse_event(&mut pending, true, WM_LBUTTONDOWN, 0, 2, 2, region),
            (true, true)
        );
        assert_eq!(
            classify_stop_mouse_event(
                &mut pending,
                false,
                WM_LBUTTONUP,
                LLMHF_INJECTED,
                2,
                2,
                None
            ),
            (false, false)
        );
        assert!(pending);
        assert_eq!(
            classify_stop_mouse_event(&mut pending, false, WM_LBUTTONUP, 0, 15, 15, None),
            (true, false)
        );
        assert!(!pending);
        assert_eq!(
            classify_stop_mouse_event(&mut pending, false, WM_LBUTTONUP, 0, 15, 15, None),
            (false, false)
        );
    }

    #[test]
    fn stop_key_rejects_all_injected_keys() {
        let key = EventKind::KeyDown {
            vk: 0x1B,
            scan: 1,
            extended: false,
        };
        assert!(is_physical_stop_key(&key, 0, 0x78, true));
        assert!(is_physical_stop_key(&key, 0, 0x78, false));
        assert!(!is_physical_stop_key(&key, LLKHF_INJECTED.0, 0x78, true));
        let f9 = EventKind::KeyDown {
            vk: 0x78,
            scan: 0,
            extended: false,
        };
        assert!(is_physical_stop_key(&f9, 0, 0x78, true));
        assert!(!is_physical_stop_key(&f9, 0, 0x78, false));
        assert!(!is_physical_stop_key(&f9, LLKHF_INJECTED.0, 0x78, true));
    }
}
