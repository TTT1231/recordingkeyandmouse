//! Best-effort, click-through visual feedback for successfully injected clicks.
//! Window operations live on a separate thread and never block the replay scheduler.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateEllipticRgn, CreateSolidBrush, DeleteObject, RGN_DIFF, SetWindowRgn,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, HWND_TOPMOST, LWA_ALPHA, MSG,
    PM_REMOVE, PeekMessageW, RegisterClassW, SW_HIDE, SWP_NOACTIVATE, SWP_SHOWWINDOW,
    SetLayeredWindowAttributes, SetWindowPos, ShowWindow, TranslateMessage, UnregisterClassW,
    WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    WS_POPUP,
};
use windows::core::w;

const DISPLAY_TIME: Duration = Duration::from_millis(420);

pub(crate) struct ClickHighlight {
    sender: mpsc::SyncSender<(i32, i32)>,
    closed: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ClickHighlight {
    pub fn start() -> Option<Self> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let closed = Arc::new(AtomicBool::new(false));
        let thread_closed = closed.clone();
        let thread = std::thread::Builder::new()
            .name("repeat-click-highlight".into())
            .spawn(move || run(receiver, &thread_closed))
            .ok()?;
        Some(Self {
            sender,
            closed,
            thread: Some(thread),
        })
    }

    /// Never block input injection if visual feedback cannot keep up.
    pub fn show(&self, x: i32, y: i32) {
        let _ = self.sender.try_send((x, y));
    }
}

impl Drop for ClickHighlight {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}

fn run(receiver: mpsc::Receiver<(i32, i32)>, closed: &AtomicBool) {
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else {
            return;
        };
        let instance: HINSTANCE = module.into();
        let brush = CreateSolidBrush(COLORREF(0x00ff_842e));
        if brush.is_invalid() {
            return;
        }
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hbrBackground: brush,
            lpszClassName: w!("RepeatClickHighlight"),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            let _ = DeleteObject(brush.into());
            return;
        }
        let window = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            w!("RepeatClickHighlight"),
            w!(""),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(instance),
            None,
        );
        if let Ok(window) = window {
            let outer = CreateEllipticRgn(0, 0, 34, 34);
            let inner = CreateEllipticRgn(3, 3, 31, 31);
            let ready = !outer.is_invalid()
                && !inner.is_invalid()
                && CombineRgn(Some(outer), Some(outer), Some(inner), RGN_DIFF).0 != 0
                && SetLayeredWindowAttributes(window, COLORREF(0), 255, LWA_ALPHA).is_ok()
                && SetWindowRgn(window, Some(outer), true) != 0;
            if !inner.is_invalid() {
                let _ = DeleteObject(inner.into());
            }
            // A successful SetWindowRgn transfers ownership to Windows.
            if !ready && !outer.is_invalid() {
                let _ = DeleteObject(outer.into());
            }
            let mut deadline = None;
            while ready && !closed.load(Ordering::Acquire) {
                if let Ok((x, y)) = receiver.recv_timeout(Duration::from_millis(16)) {
                    if closed.load(Ordering::Acquire) {
                        break;
                    }
                    let _ = SetWindowPos(
                        window,
                        Some(HWND_TOPMOST),
                        x - 17,
                        y - 17,
                        34,
                        34,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                    deadline = Some(Instant::now() + DISPLAY_TIME);
                }
                if deadline.is_some_and(|at| Instant::now() >= at) {
                    let _ = ShowWindow(window, SW_HIDE);
                    deadline = None;
                }
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            let _ = DestroyWindow(window);
        }
        let _ = UnregisterClassW(w!("RepeatClickHighlight"), Some(instance));
        let _ = DeleteObject(brush.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventKind, MouseButton};
    use crate::scheduler::{ReplayDriver, StopSignal};
    use std::sync::atomic::AtomicU32;
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{GetDC, GetPixel, ReleaseDC};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, SetCursorPos, SetForegroundWindow, WM_LBUTTONDOWN, WM_LBUTTONUP,
    };

    static DOWN: AtomicU32 = AtomicU32::new(0);
    static UP: AtomicU32 = AtomicU32::new(0);

    unsafe extern "system" fn receiver_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        if msg == WM_LBUTTONDOWN {
            DOWN.fetch_add(1, Ordering::Relaxed);
        }
        if msg == WM_LBUTTONUP {
            UP.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { DefWindowProcW(hwnd, msg, w, l) }
    }

    /// Real interactive-desktop check: paint pixels, deliver injected down/up through the
    /// coloured stroke to a dedicated window, and confirm cleanup removes the stroke.
    #[test]
    #[ignore = "会在交互桌面显示专用窗口并注入一次固定点击"]
    fn native_click_highlight_pixels_and_passthrough() {
        crate::platform::init_process_dpi();
        let geometry = crate::platform::snapshot_geometry();
        let monitor = geometry.monitors.first().expect("无法获取显示器");
        let (x, y) = (
            monitor.x + monitor.width / 2,
            monitor.y + monitor.height / 2,
        );
        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None).unwrap().into();
            let background = CreateSolidBrush(COLORREF(0x0000_0000));
            let class = WNDCLASSW {
                lpfnWndProc: Some(receiver_proc),
                hInstance: instance,
                hbrBackground: background,
                lpszClassName: w!("RepeatHighlightReceiver"),
                ..Default::default()
            };
            assert_ne!(RegisterClassW(&class), 0);
            let receiver = CreateWindowExW(
                WS_EX_TOPMOST,
                w!("RepeatHighlightReceiver"),
                w!("Repeat 测试接收器"),
                WS_POPUP,
                x - 100,
                y - 100,
                200,
                200,
                None,
                None,
                Some(instance),
                None,
            )
            .unwrap();
            let _ = SetWindowPos(
                receiver,
                Some(HWND_TOPMOST),
                x - 100,
                y - 100,
                200,
                200,
                SWP_SHOWWINDOW | SWP_NOACTIVATE,
            );
            let _ = SetForegroundWindow(receiver);
            let mut old_cursor = POINT::default();
            let _ = GetCursorPos(&mut old_cursor);
            DOWN.store(0, Ordering::Relaxed);
            UP.store(0, Ordering::Relaxed);
            let highlight = ClickHighlight::start().expect("视觉线程启动失败");
            highlight.show(x, y);
            let sample = (x, y - 16);
            let mut msg = MSG::default();
            let until = Instant::now() + Duration::from_millis(300);
            while Instant::now() < until {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    DispatchMessageW(&msg);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let dc = GetDC(None);
            let before = GetPixel(dc, sample.0, sample.1);
            let center = GetPixel(dc, x, y);
            let corner = GetPixel(dc, x - 16, y - 16);
            let _ = ReleaseDC(None, dc);
            let stop = StopSignal::new();
            let mut driver = crate::platform::make_driver(&stop, &geometry).unwrap();
            for kind in [
                EventKind::MouseMove {
                    x: sample.0,
                    y: sample.1,
                },
                EventKind::MouseDown {
                    button: MouseButton::Left,
                    x: sample.0,
                    y: sample.1,
                },
                EventKind::MouseUp {
                    button: MouseButton::Left,
                    x: sample.0,
                    y: sample.1,
                },
            ] {
                driver.inject(&kind).unwrap();
            }
            let until = Instant::now() + Duration::from_millis(150);
            while Instant::now() < until {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    DispatchMessageW(&msg);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            drop(highlight);
            let dc = GetDC(None);
            let after = GetPixel(dc, sample.0, sample.1);
            let _ = ReleaseDC(None, dc);
            let count = (DOWN.load(Ordering::Relaxed), UP.load(Ordering::Relaxed));
            let _ = SetCursorPos(old_cursor.x, old_cursor.y);
            let _ = DestroyWindow(receiver);
            let _ = UnregisterClassW(w!("RepeatHighlightReceiver"), Some(instance));
            let _ = DeleteObject(background.into());
            println!(
                "HIGHLIGHT_NATIVE before={before:?} after={after:?} down={} up={}",
                count.0, count.1
            );
            let evidence = serde_json::json!({
                "fixture": "native click-through highlight above dedicated receiver",
                "highlightAtPhysical": {"x": x, "y": y},
                "injectedAtPhysical": {"x": sample.0, "y": sample.1},
                "strokeColor": before.0,
                "centerColor": center.0,
                "cornerColor": corner.0,
                "pixelAfterHide": after.0,
                "receiverEvents": {"WM_LBUTTONDOWN": count.0, "WM_LBUTTONUP": count.1}
            });
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target/click-highlight-evidence.json");
            std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
            assert_eq!(before, COLORREF(0x00ff_842e), "高亮像素应为蓝色");
            assert_ne!(center, before, "圆环中心应透明");
            assert_ne!(corner, before, "圆环角落应透明");
            assert_ne!(after, before, "停止后高亮应消失");
            assert_eq!(count, (1, 1), "点击应穿透到接收器");
        }
    }
}
