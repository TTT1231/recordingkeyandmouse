//! Real desktop pixels, native encoding and capture exclusion; fixed colors only.
#![cfg(windows)]
use repeat_lib::{
    capture::RecordingBuffer, event::EventKind, platform, screen_capture::ScreenCapture,
};
use std::{
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::w,
};

static TARGET_KEY_MESSAGES: AtomicU32 = AtomicU32::new(0);
static TARGET_MOUSE_MESSAGES: AtomicU32 = AtomicU32::new(0);

unsafe extern "system" fn paint(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut ps);
                let brush = if GetWindowLongPtrW(hwnd, GWLP_USERDATA) == 1 {
                    WHITE_BRUSH
                } else {
                    BLACK_BRUSH
                };
                FillRect(dc, &ps.rcPaint, HBRUSH(GetStockObject(brush).0));
                // Distinct colors at distinct heights also detect RGB channel
                // swaps and vertically inverted native encoder input.
                for (top, color) in [(40, COLORREF(0x0000ff)), (220, COLORREF(0xff0000))] {
                    let brush = CreateSolidBrush(color);
                    FillRect(
                        dc,
                        &RECT {
                            left: 280,
                            top,
                            right: 440,
                            bottom: top + 100,
                        },
                        brush,
                    );
                    let _ = DeleteObject(brush.into());
                }
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_KEYDOWN if wp.0 == 0x75 => {
                TARGET_KEY_MESSAGES.fetch_add(1, Ordering::SeqCst);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 1);
                let _ = InvalidateRect(Some(hwnd), None, false);
                let _ = UpdateWindow(hwnd);
                LRESULT(0)
            }
            WM_KEYUP if wp.0 == 0x75 => {
                TARGET_KEY_MESSAGES.fetch_add(1, Ordering::SeqCst);
                LRESULT(0)
            }
            WM_LBUTTONDOWN | WM_LBUTTONUP => {
                TARGET_MOUSE_MESSAGES.fetch_add(1, Ordering::SeqCst);
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

struct Fixture {
    hwnd: usize,
    worker: Option<thread::JoinHandle<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(self.hwnd as *mut _)),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            );
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[test]
#[ignore = "显示彩色测试窗口，发送固定 F6 改变画面，需交互桌面和 ffmpeg"]
fn native_video_records_changes_and_excludes_overlay() {
    platform::init_process_dpi();
    let (tx, rx) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let class = w!("RepeatScreenVideoFixture");
        assert_ne!(
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(paint),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            }),
            0
        );
        let base = CreateWindowExW(
            WS_EX_TOPMOST,
            class,
            w!("Repeat 画面测试"),
            WS_POPUP | WS_VISIBLE,
            40,
            40,
            480,
            360,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap();
        let overlay = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            class,
            w!("Repeat 排除测试"),
            WS_POPUP | WS_VISIBLE,
            80,
            80,
            160,
            120,
            Some(base),
            None,
            Some(instance.into()),
            None,
        )
        .unwrap();
        SetWindowLongPtrW(overlay, GWLP_USERDATA, 1);
        SetWindowDisplayAffinity(overlay, WDA_EXCLUDEFROMCAPTURE).unwrap();
        let _ = InvalidateRect(Some(overlay), None, false);
        let _ = UpdateWindow(base);
        let _ = UpdateWindow(overlay);
        tx.send(base.0 as usize).unwrap();
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    });
    let fixture = Fixture {
        hwnd: rx.recv_timeout(Duration::from_secs(10)).unwrap(),
        worker: Some(worker),
    };
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("screen-pixels-{}", repeat_lib::document::new_id()));
    let path = directory.join("capture.mp4");
    let capture = ScreenCapture::prepare(&path, &platform::snapshot_geometry()).unwrap();
    let service = platform::start_input_service(Arc::new(|_| {})).unwrap();
    let buffer = Arc::new(Mutex::new(RecordingBuffer::default()));
    let start = Instant::now();
    buffer.lock().unwrap().begin_at(start);
    service.start_capture(buffer.clone()).unwrap();
    capture.start(start).unwrap();
    thread::sleep(Duration::from_millis(700));
    unsafe {
        let target = HWND(fixture.hwnd as *mut _);
        assert_eq!(WindowFromPoint(POINT { x: 400, y: 250 }), target);
        // A fixed click activates only this verified test window. Windows can
        // reject SetForegroundWindow from a background test process.
        let x = (400 - GetSystemMetrics(SM_XVIRTUALSCREEN)) * 65535
            / (GetSystemMetrics(SM_CXVIRTUALSCREEN) - 1);
        let y = (250 - GetSystemMetrics(SM_YVIRTUALSCREEN)) * 65535
            / (GetSystemMetrics(SM_CYVIRTUALSCREEN) - 1);
        let mouse = [
            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
            MOUSEEVENTF_LEFTDOWN,
            MOUSEEVENTF_LEFTUP,
        ]
        .map(|flags| INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: x,
                    dy: y,
                    dwFlags: flags,
                    dwExtraInfo: 0x56494445,
                    ..Default::default()
                },
            },
        });
        assert_eq!(SendInput(&mouse, size_of::<INPUT>() as i32), 3);
        thread::sleep(Duration::from_millis(50));
        assert_eq!(GetForegroundWindow(), target, "固定输入必须进入测试窗口");
        let inputs = [false, true].map(|up| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0x75),
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        Default::default()
                    },
                    dwExtraInfo: 0x56494445,
                    ..Default::default()
                },
            },
        });
        assert_eq!(SendInput(&inputs, size_of::<INPUT>() as i32), 2);
    }
    thread::sleep(Duration::from_millis(900));
    service.stop_capture().unwrap();
    let recorded = buffer.lock().unwrap().finish();
    service.shutdown();
    let info = capture.finish(recorded.duration_us).unwrap();
    drop(fixture);
    assert_eq!(TARGET_KEY_MESSAGES.load(Ordering::SeqCst), 2);
    assert_eq!(TARGET_MOUSE_MESSAGES.load(Ordering::SeqCst), 2);
    let key_events: Vec<_> = recorded
        .events
        .iter()
        .filter(|event| event.kind.vk() == Some(0x75))
        .collect();
    assert_eq!(key_events.len(), 2);
    assert!(matches!(key_events[0].kind, EventKind::KeyDown { .. }));
    assert!(matches!(key_events[1].kind, EventKind::KeyUp { .. }));
    assert!(repeat_lib::document::validate_balanced(&recorded.events).is_ok());
    let mouse_events: Vec<_> = recorded
        .events
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                EventKind::MouseDown { x: 400, y: 250, .. }
                    | EventKind::MouseUp { x: 400, y: 250, .. }
            )
        })
        .collect();
    assert_eq!(mouse_events.len(), 2);
    let pixel = |time: &str, px: i32, py: i32| {
        let x = ((px - info.origin_x) as f64 * f64::from(info.width)
            / f64::from(info.capture_width))
        .round() as u32;
        let y = ((py - info.origin_y) as f64 * f64::from(info.height)
            / f64::from(info.capture_height))
        .round() as u32;
        let output = Command::new("ffmpeg")
            .args(["-v", "error", "-ss", time, "-i"])
            .arg(&path)
            .args([
                "-frames:v",
                "1",
                "-vf",
                &format!("crop=2:2:{x}:{y}"),
                "-pix_fmt",
                "rgb24",
                "-f",
                "rawvideo",
                "-",
            ])
            .output()
            .expect("此测试需 ffmpeg，产品录制不依赖它");
        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 12);
        let mut rgb = [0u32; 3];
        for pixel in output.stdout.as_chunks::<3>().0 {
            for (channel, value) in pixel.iter().enumerate() {
                rgb[channel] += u32::from(*value);
            }
        }
        rgb.map(|sum| sum / 4)
    };
    let before = pixel("0.2", 120, 120);
    let after = pixel("1.4", 120, 120);
    let red = pixel("0.2", 360, 120);
    let blue = pixel("0.2", 360, 320);
    assert!(
        before.iter().all(|value| *value < 30),
        "白色悬浮窗应排除：{before:?}"
    );
    assert!(
        after.iter().all(|value| *value > 225),
        "应保留 F6 引起的画面变化：{after:?}"
    );
    assert!(
        red[0] > 225 && red[1] < 30 && red[2] < 30,
        "上方红色色块：{red:?}"
    );
    assert!(
        blue[2] > 225 && blue[0] < 30 && blue[1] < 30,
        "下方蓝色色块：{blue:?}"
    );
    let evidence = serde_json::json!({"capture":info,"beforeRgb":before,"afterRgb":after,"upperRedRgb":red,"lowerBlueRgb":blue,"fixedKeyEvents":key_events,"fixedMouseEvents":mouse_events,"targetKeyMessages":TARGET_KEY_MESSAGES.load(Ordering::SeqCst),"targetMouseMessages":TARGET_MOUSE_MESSAGES.load(Ordering::SeqCst),"excludedOverlayVisibleOnDesktop":true,"video":path});
    std::fs::write(
        directory.join("pixel-evidence.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
    println!("SCREEN_VIDEO_PIXEL_EVIDENCE {evidence}");
}
