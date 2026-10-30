//! Best-effort, click-through visual feedback for successfully injected clicks.
//! Window operations live on a separate thread and never block the replay scheduler.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, HWND_TOPMOST, MSG, PM_REMOVE,
    PeekMessageW, RegisterClassW, SW_HIDE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetWindowPos,
    ShowWindow, TranslateMessage, ULW_ALPHA, UnregisterClassW, UpdateLayeredWindow, WNDCLASSW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::w;

const DISPLAY_TIME: Duration = Duration::from_millis(420);
const HIGHLIGHT_SIZE: i32 = 36;

/// Premultiplied BGRA with one physical pixel of coverage at either ring edge.
/// The hollow center and padded corners remain fully transparent.
fn ring_pixels() -> Vec<u8> {
    let mut pixels = Vec::with_capacity((HIGHLIGHT_SIZE * HIGHLIGHT_SIZE * 4) as usize);
    let center = f64::from(HIGHLIGHT_SIZE) / 2.0;
    for y in 0..HIGHLIGHT_SIZE {
        for x in 0..HIGHLIGHT_SIZE {
            let distance = (f64::from(x) + 0.5 - center).hypot(f64::from(y) + 0.5 - center);
            let coverage = (17.5 - distance).min(distance - 13.5).clamp(0.0, 1.0);
            let alpha = (coverage * 255.0).round() as u16;
            pixels.extend_from_slice(&[
                alpha as u8,
                ((132 * alpha + 127) / 255) as u8,
                ((46 * alpha + 127) / 255) as u8,
                alpha as u8,
            ]);
        }
    }
    pixels
}

struct HighlightSurface {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
}

impl HighlightSurface {
    fn new() -> Option<Self> {
        let mut surface = Self {
            dc: HDC::default(),
            bitmap: HBITMAP::default(),
            previous: HGDIOBJ::default(),
        };
        unsafe {
            surface.dc = CreateCompatibleDC(None);
            if surface.dc.is_invalid() {
                return None;
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: HIGHLIGHT_SIZE,
                    biHeight: -HIGHLIGHT_SIZE,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut destination = std::ptr::null_mut();
            surface.bitmap = CreateDIBSection(
                Some(surface.dc),
                &info,
                DIB_RGB_COLORS,
                &mut destination,
                None,
                0,
            )
            .ok()?;
            surface.previous = SelectObject(surface.dc, surface.bitmap.into());
            if surface.previous.is_invalid() || destination.is_null() {
                return None;
            }
            let pixels = ring_pixels();
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), destination.cast::<u8>(), pixels.len());
        }
        Some(surface)
    }

    fn show(&self, window: HWND, x: i32, y: i32) -> windows::core::Result<()> {
        let position = POINT {
            x: x - HIGHLIGHT_SIZE / 2,
            y: y - HIGHLIGHT_SIZE / 2,
        };
        let size = SIZE {
            cx: HIGHLIGHT_SIZE,
            cy: HIGHLIGHT_SIZE,
        };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        unsafe {
            UpdateLayeredWindow(
                window,
                None,
                Some(&position),
                Some(&size),
                Some(self.dc),
                Some(&POINT::default()),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )?;
            SetWindowPos(
                window,
                Some(HWND_TOPMOST),
                position.x,
                position.y,
                size.cx,
                size.cy,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        }
    }
}

impl Drop for HighlightSurface {
    fn drop(&mut self) {
        unsafe {
            if !self.previous.is_invalid() {
                SelectObject(self.dc, self.previous);
            }
            if !self.bitmap.is_invalid() {
                let _ = DeleteObject(self.bitmap.into());
            }
            if !self.dc.is_invalid() {
                let _ = DeleteDC(self.dc);
            }
        }
    }
}

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
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: w!("RepeatClickHighlight"),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
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
            let surface = HighlightSurface::new();
            let mut deadline = None;
            while let Some(surface) = surface.as_ref()
                && !closed.load(Ordering::Acquire)
            {
                if let Ok((x, y)) = receiver.recv_timeout(Duration::from_millis(16)) {
                    if closed.load(Ordering::Acquire) {
                        break;
                    }
                    if surface.show(window, x, y).is_err() {
                        break;
                    }
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

    unsafe fn snapshot_ring(dc: HDC, x: i32, y: i32) -> Vec<u8> {
        unsafe {
            let memory = CreateCompatibleDC(None);
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: 48,
                    biHeight: -48,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap =
                CreateDIBSection(Some(memory), &info, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
            let previous = SelectObject(memory, bitmap.into());
            let copied = BitBlt(
                memory,
                0,
                0,
                48,
                48,
                Some(dc),
                x - 24,
                y - 24,
                SRCCOPY | CAPTUREBLT,
            );
            let _ = GdiFlush();
            let rgba = std::slice::from_raw_parts(bits.cast::<u8>(), 48 * 48 * 4);
            let pixels = rgba
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|pixel| [pixel[2], pixel[1], pixel[0]])
                .collect();
            SelectObject(memory, previous);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(memory);
            copied.unwrap();
            pixels
        }
    }

    #[test]
    fn ring_has_smooth_premultiplied_edges_and_clear_center() {
        let pixels = ring_pixels();
        let at = |x: usize, y: usize| {
            let start = (y * HIGHLIGHT_SIZE as usize + x) * 4;
            &pixels[start..start + 4]
        };
        assert_eq!(at(18, 18), &[0, 0, 0, 0]);
        assert_eq!(at(0, 0), &[0, 0, 0, 0]);
        assert_eq!(at(18, 2), &[255, 132, 46, 255]);
        assert!(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[3] > 0 && pixel[3] < 255)
        );
        for y in 0..HIGHLIGHT_SIZE as usize {
            for x in 0..HIGHLIGHT_SIZE as usize {
                let pixel = at(x, y);
                assert!(pixel[..3].iter().all(|channel| *channel <= pixel[3]));
                assert_eq!(pixel, at(HIGHLIGHT_SIZE as usize - 1 - x, y));
                assert_eq!(pixel, at(x, HIGHLIGHT_SIZE as usize - 1 - y));
            }
        }
    }

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
            let pixels = snapshot_ring(dc, x, y);
            let blended_edge_pixels = pixels
                .as_chunks::<3>()
                .0
                .iter()
                .filter(|pixel| **pixel != [0, 0, 0] && **pixel != [46, 132, 255])
                .count();
            let _ = ReleaseDC(None, dc);
            // Keep the receiver's message loop responsive to mouse activation
            // while SendInput runs, just as it is with a real target app.
            let injection_geometry = geometry.clone();
            let injection = std::thread::spawn(move || {
                let stop = StopSignal::new();
                let mut driver = crate::platform::make_driver(&stop, &injection_geometry).unwrap();
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
            });
            let until = Instant::now() + Duration::from_secs(5);
            while !injection.is_finished() && Instant::now() < until {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    DispatchMessageW(&msg);
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            assert!(injection.is_finished(), "固定输入超时");
            injection.join().unwrap();
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
                "blendedEdgePixels": blended_edge_pixels,
                "receiverEvents": {"WM_LBUTTONDOWN": count.0, "WM_LBUTTONUP": count.1}
            });
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target/click-highlight-evidence.json");
            std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
            std::fs::write("target/click-highlight-rgb24.raw", pixels).unwrap();
            assert_eq!(before, COLORREF(0x00ff_842e), "高亮像素应为蓝色");
            assert_ne!(center, before, "圆环中心应透明");
            assert_ne!(corner, before, "圆环角落应透明");
            assert_ne!(after, before, "停止后高亮应消失");
            assert_eq!(count, (1, 1), "点击应穿透到接收器");
            assert!(
                blended_edge_pixels > 0,
                "圆环边缘应包含透明度过渡，不能是二值裁切"
            );
        }
    }
}
