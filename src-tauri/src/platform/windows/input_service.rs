//! Windows 输入服务线程：
//! - 专用线程安装 `WH_KEYBOARD_LL` / `WH_MOUSE_LL` 并维持消息循环（M1-03）；
//! - 回调只复制必要字段、读时间戳、写入有界缓冲，随即 CallNextHookEx 继续钩子链；
//! - 同一线程注册全局热键 F8（启停录制）/ F9（停止执行），窗口失焦仍可触发（M1-05）；
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
    CallNextHookEx, DispatchMessageW, GetMessageW, HHOOK, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, MSG,
    MSLLHOOKSTRUCT, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL,
    WH_MOUSE_LL, WM_APP, WM_HOTKEY, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
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
    mouse_hook: Option<HHOOK>,
    buffer: Option<Arc<Mutex<RecordingBuffer>>>,
    on_hotkey: Option<Arc<dyn Fn(HotkeyAction) + Send + Sync>>,
}

thread_local! {
    static STATE: RefCell<ThreadState> = RefCell::new(ThreadState {
        keyboard_hook: None,
        execution_active: false,
        stop_vk: 0x78,
        mouse_hook: None,
        buffer: None,
        on_hotkey: None,
    });
}

pub struct InputServiceHandle {
    thread_id: u32,
    hotkeys_ready: Arc<AtomicBool>,
    queue: Arc<Mutex<VecDeque<ServiceCommand>>>,
    status: Arc<Mutex<Vec<String>>>,
    join: Mutex<Option<std::thread::JoinHandle<()>>>,
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

    pub fn start_capture(&self, buffer: Arc<Mutex<RecordingBuffer>>) -> Result<(), String> {
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

    pub fn arm_execution(&self, active: bool) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.send_cmd(ServiceCommand::ArmExecution(active, tx))?;
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "启用原生停止监听超时")?
    }
    pub fn ensure_stop_available(&self) -> Result<(), String> {
        if self.hotkeys_ready.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err("全局停止快捷键未注册，请先在帮助菜单中更换未被占用的快捷键".into())
        }
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
    let hotkeys_ready = Arc::new(AtomicBool::new(false));
    let ready_for_thread = hotkeys_ready.clone();
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
        hotkeys_ready,
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
    hotkeys_ready: Arc<AtomicBool>,
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
        {
            let mut st = status.lock().unwrap();
            let mut registered = 0;
            match RegisterHotKey(None, HOTKEY_ID_TOGGLE_RECORD, MOD_NOREPEAT, VK_F8 as u32) {
                Ok(()) => {
                    registered += 1;
                    st.push("F8 启停录制：已注册（全局生效）".into());
                }
                Err(e) => st.push(format!(
                    "F8 启停录制：注册失败（{e}），可能被其他程序占用；可继续使用界面按钮"
                )),
            }
            match RegisterHotKey(None, HOTKEY_ID_STOP_EXEC, MOD_NOREPEAT, VK_F9 as u32) {
                Ok(()) => {
                    registered += 1;
                    st.push("F9 停止执行：已注册（全局生效）".into());
                }
                Err(e) => st.push(format!(
                    "F9 停止执行：注册失败（{e}），可能被其他程序占用；可继续使用界面按钮"
                )),
            }
            hotkeys_ready.store(registered == 2, Ordering::SeqCst);
        }

        let _ = ready_tx.send(Ok(tid));

        let mut hotkeys = crate::capture::Hotkeys::default();
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
                        ServiceCommand::StartCapture(buffer) => match install_hooks() {
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
                            let result =
                                install_hooks().map_err(|e| format!("安装输入钩子失败：{e}"));
                            if result.is_ok() {
                                STATE.with(|s| s.borrow_mut().buffer = Some(buffer));
                            }
                            let _ = reply.send(result);
                        }
                        ServiceCommand::StopCaptureChecked(reply) => {
                            stop_capture_hooks();
                            let _ = reply.send(Ok(()));
                        }
                        ServiceCommand::SetHotkeys(next, reply) => {
                            let result = configure_hotkeys(&hotkeys, &next, &hotkeys_ready);
                            if result.is_ok() {
                                hotkeys = next;
                                STATE.with(|s| s.borrow_mut().stop_vk = hotkeys.stop_vk);
                                *status.lock().unwrap() = vec![format!(
                                    "{} 启停录制；{} 停止执行",
                                    crate::event::vk_name(hotkeys.record_vk),
                                    crate::event::vk_name(hotkeys.stop_vk)
                                )];
                            }
                            let _ = reply.send(result);
                        }
                        ServiceCommand::ArmExecution(active, reply) => {
                            let result = if active {
                                install_keyboard_hook()
                                    .map_err(|e| format!("无法监听执行停止键：{e}"))
                            } else {
                                Ok(())
                            };
                            if result.is_ok() {
                                STATE.with(|s| s.borrow_mut().execution_active = active);
                            }
                            let _ = reply.send(result);
                        }
                        ServiceCommand::Shutdown => {
                            expected_shutdown = true;
                            break 'main;
                        }
                    }
                },
                WM_HOTKEY => {
                    let action = match msg.wParam.0 as i32 {
                        HOTKEY_ID_TOGGLE_RECORD => Some(HotkeyAction::ToggleRecording),
                        HOTKEY_ID_STOP_EXEC => Some(HotkeyAction::StopExecution),
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

        hotkeys_ready.store(false, Ordering::SeqCst);
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
fn install_hooks() -> windows::core::Result<()> {
    stop_capture_hooks();
    install_keyboard_hook()?;
    unsafe {
        let module = GetModuleHandleW(None)?;
        let hook = SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(low_level_mouse_proc),
            Some(HINSTANCE(module.0)),
            0,
        )?;
        STATE.with(|s| s.borrow_mut().mouse_hook = Some(hook));
    }
    Ok(())
}
fn stop_capture_hooks() {
    let mouse = STATE.with(|s| {
        let mut st = s.borrow_mut();
        st.buffer = None;
        st.mouse_hook.take()
    });
    if let Some(hook) = mouse {
        unsafe {
            let _ = UnhookWindowsHookEx(hook);
        }
    }
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
                let stop = st.execution_active
                    && matches!(kind,EventKind::KeyDown{vk,..} if vk==st.stop_vk);
                (
                    st.buffer.clone(),
                    if stop { st.on_hotkey.clone() } else { None },
                )
            });
            if let Some(callback) = callback {
                callback(HotkeyAction::StopExecution);
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

fn configure_hotkeys(
    old: &crate::capture::Hotkeys,
    next: &crate::capture::Hotkeys,
    ready: &AtomicBool,
) -> Result<(), String> {
    next.validate()?;
    ready.store(false, Ordering::SeqCst);
    unsafe {
        let _ = UnregisterHotKey(None, HOTKEY_ID_TOGGLE_RECORD);
        let _ = UnregisterHotKey(None, HOTKEY_ID_STOP_EXEC);
        let result = RegisterHotKey(None, HOTKEY_ID_TOGGLE_RECORD, MOD_NOREPEAT, next.record_vk)
            .and_then(|_| RegisterHotKey(None, HOTKEY_ID_STOP_EXEC, MOD_NOREPEAT, next.stop_vk));
        if let Err(error) = result {
            let _ = UnregisterHotKey(None, HOTKEY_ID_TOGGLE_RECORD);
            let _ = UnregisterHotKey(None, HOTKEY_ID_STOP_EXEC);
            let record_ok =
                RegisterHotKey(None, HOTKEY_ID_TOGGLE_RECORD, MOD_NOREPEAT, old.record_vk).is_ok();
            let stop_ok =
                RegisterHotKey(None, HOTKEY_ID_STOP_EXEC, MOD_NOREPEAT, old.stop_vk).is_ok();
            ready.store(record_ok && stop_ok, Ordering::SeqCst);
            return Err(format!(
                "快捷键被占用，设置未保存：{error}。请换用其他快捷键"
            ));
        }
    }
    ready.store(true, Ordering::SeqCst);
    Ok(())
}
