//! 接收本会话锁定、断开、挂起和显示布局变化；通知直接唤醒原生停止通路。
use crate::platform::HotkeyAction;
use std::{cell::RefCell, sync::Arc};
use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        System::{
            LibraryLoader::GetModuleHandleW,
            RemoteDesktop::{
                NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification,
                WTSUnRegisterSessionNotification,
            },
        },
        UI::WindowsAndMessaging::*,
    },
    core::w,
};
type Callback = Arc<dyn Fn(HotkeyAction) + Send + Sync>;
thread_local! { static CALLBACK: RefCell<Option<Callback>> = RefCell::new(None); }
pub struct EnvironmentWatch {
    hwnd: HWND,
    instance: HINSTANCE,
}
impl EnvironmentWatch {
    pub fn new(callback: Arc<dyn Fn(HotkeyAction) + Send + Sync>) -> Result<Self, String> {
        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None).map_err(|e| e.to_string())?.into();
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: w!("RepeatInputMonitor"),
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 {
                return Err(format!(
                    "无法注册会话监视窗口：{}",
                    windows::core::Error::from_win32()
                ));
            }
            let hwnd = match CreateWindowExW(
                Default::default(),
                w!("RepeatInputMonitor"),
                w!("Repeat input monitor"),
                Default::default(),
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance),
                None,
            ) {
                Ok(hwnd) => hwnd,
                Err(e) => {
                    let _ = UnregisterClassW(w!("RepeatInputMonitor"), Some(instance));
                    return Err(format!("无法监视桌面会话：{e}"));
                }
            };
            if let Err(e) = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) {
                let _ = DestroyWindow(hwnd);
                let _ = UnregisterClassW(w!("RepeatInputMonitor"), Some(instance));
                return Err(format!("无法监视会话切换：{e}"));
            }
            CALLBACK.with(|cb| *cb.borrow_mut() = Some(callback));
            Ok(Self { hwnd, instance })
        }
    }
}
impl Drop for EnvironmentWatch {
    fn drop(&mut self) {
        unsafe {
            CALLBACK.with(|cb| *cb.borrow_mut() = None);
            let _ = WTSUnRegisterSessionNotification(self.hwnd);
            let _ = DestroyWindow(self.hwnd);
            let _ = UnregisterClassW(w!("RepeatInputMonitor"), Some(self.instance));
        }
    }
}
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let interrupted = msg == WM_DISPLAYCHANGE
        || (msg == WM_POWERBROADCAST
            && matches!(
                wparam.0 as u32,
                PBT_APMSUSPEND | PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND
            ))
        || (msg == WM_WTSSESSION_CHANGE
            && matches!(
                wparam.0 as u32,
                WTS_SESSION_LOCK
                    | WTS_SESSION_LOGOFF
                    | WTS_CONSOLE_DISCONNECT
                    | WTS_REMOTE_DISCONNECT
            ));
    if interrupted {
        let callback = CALLBACK.with(|cb| cb.borrow().clone());
        if let Some(callback) = callback {
            callback(HotkeyAction::EnvironmentChanged);
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
