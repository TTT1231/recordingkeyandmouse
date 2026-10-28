//! 隔离原生诊断：比较屏幕左边缘的低级钩子坐标与系统光标位置。
#![cfg(windows)]

use std::{cell::RefCell, sync::mpsc, thread, time::Duration};
use windows::Win32::{
    Foundation::{HINSTANCE, LPARAM, LRESULT, POINT, WPARAM},
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::{
        Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_MOVE, MOUSEINPUT, SendInput,
        },
        WindowsAndMessaging::{
            CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
            GetCursorPos, GetMessageW, HHOOK, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT,
            PostThreadMessageW, RegisterClassW, SetCursorPos, SetWindowsHookExW,
            UnhookWindowsHookEx, UnregisterClassW, WH_MOUSE_LL, WM_MOUSEMOVE, WM_QUIT, WNDCLASSW,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
        },
    },
};
use windows::core::w;

// Use a distinct 32-bit marker so the application's own input filter does not
// suppress the diagnostic sample and the marker survives cross-process hooks.
const MARKER: usize = 0x5250_5445;
type HookSample = (i32, i32, u32, u32, bool);
thread_local! { static SAMPLE: RefCell<Option<mpsc::Sender<HookSample>>> = const { RefCell::new(None) }; }

unsafe extern "system" fn mouse_hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code >= 0 && w.0 as u32 == WM_MOUSEMOVE {
        let event = unsafe { &*(l.0 as *const MSLLHOOKSTRUCT) };
        if event.flags & LLMHF_INJECTED != 0 {
            SAMPLE.with(|slot| {
                if let Some(tx) = slot.borrow().as_ref() {
                    let _ = tx.send((
                        event.pt.x,
                        event.pt.y,
                        event.flags,
                        event.time,
                        event.dwExtraInfo == MARKER,
                    ));
                }
            });
        }
    }
    unsafe { CallNextHookEx(None, code, w, l) }
}

unsafe extern "system" fn receiver_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}

struct Probe {
    id: u32,
    join: Option<thread::JoinHandle<()>>,
}
impl Drop for Probe {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
struct CursorRestore(POINT);
impl Drop for CursorRestore {
    fn drop(&mut self) {
        unsafe {
            let _ = SetCursorPos(self.0.x, self.0.y);
        }
    }
}

#[test]
#[ignore = "移动指针到主屏左缘并自动恢复；对照真实 Windows 低级钩子和光标位置"]
fn native_injected_left_edge_hook_coordinates() {
    repeat_lib::platform::init_process_dpi();
    let geometry = repeat_lib::platform::snapshot_geometry();
    let screen = geometry
        .monitors
        .iter()
        .find(|m| m.x <= 0 && m.y <= 0 && m.x + m.width > 0 && m.y + m.height > 0)
        .expect("无法获取主屏");
    let mut original = POINT::default();
    unsafe {
        GetCursorPos(&mut original).unwrap();
    }
    let _restore = CursorRestore(original);
    let (samples_tx, samples_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (window_x, window_y) = (screen.x, screen.y + screen.height / 2 - 80);
    let join = thread::spawn(move || unsafe {
        SAMPLE.with(|slot| *slot.borrow_mut() = Some(samples_tx));
        let module = GetModuleHandleW(None).unwrap();
        let instance = HINSTANCE(module.0);
        let class = WNDCLASSW {
            lpfnWndProc: Some(receiver_proc),
            hInstance: instance,
            lpszClassName: w!("RepeatEdgeDiagnostic"),
            ..Default::default()
        };
        assert_ne!(RegisterClassW(&class), 0);
        let window = CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            w!("RepeatEdgeDiagnostic"),
            w!("Repeat 屏幕边缘诊断"),
            WS_POPUP | WS_VISIBLE,
            window_x,
            window_y,
            220,
            160,
            None,
            None,
            Some(instance),
            None,
        )
        .unwrap();
        let hook: HHOOK =
            SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(HINSTANCE(module.0)), 0).unwrap();
        ready_tx.send(GetCurrentThreadId()).unwrap();
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            DispatchMessageW(&msg);
        }
        let _ = UnhookWindowsHookEx(hook);
        let _ = DestroyWindow(window);
        let _ = UnregisterClassW(w!("RepeatEdgeDiagnostic"), Some(instance));
    });
    let _probe = Probe {
        id: ready_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        join: Some(join),
    };
    let y = screen.y + screen.height / 2;
    unsafe {
        SetCursorPos(screen.x, y).unwrap();
    }
    thread::sleep(Duration::from_millis(100));
    let mut evidence = Vec::new();
    for dx in [2, -1, -3, -20] {
        let mut before = POINT::default();
        unsafe {
            GetCursorPos(&mut before).unwrap();
        }
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx,
                    dwFlags: MOUSEEVENTF_MOVE,
                    dwExtraInfo: MARKER,
                    ..Default::default()
                },
            },
        };
        assert_eq!(
            unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) },
            1
        );
        // Windows may suppress a relative move that cannot change the cursor
        // position at the boundary; that absence is itself diagnostic evidence.
        let deadline = std::time::Instant::now() + Duration::from_millis(400);
        let hook = loop {
            let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) else {
                break None;
            };
            match samples_rx.recv_timeout(remaining) {
                Ok(sample) if sample.4 => break Some(sample),
                Ok(_) => continue,
                Err(_) => break None,
            }
        };
        thread::sleep(Duration::from_millis(40));
        let mut after = POINT::default();
        unsafe {
            GetCursorPos(&mut after).unwrap();
        }
        evidence.push(serde_json::json!({"relativeDx":dx,"before":{"x":before.x,"y":before.y},"hook":hook.map(|v| serde_json::json!({"x":v.0,"y":v.1,"flags":v.2,"tickMs":v.3,"fixedSample":v.4})),"after":{"x":after.x,"y":after.y}}));
    }
    let observed = evidence.iter().any(|e| e["hook"]["fixedSample"] == true);
    let result = serde_json::json!({"fixture":"SendInput relative mouse moves at primary screen left edge, not physical hardware","valid":observed,"monitor":{"x":screen.x,"y":screen.y,"width":screen.width,"height":screen.height,"dpi":screen.dpi},"events":evidence});
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/left-edge-hook-evidence.json");
    std::fs::write(path, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    println!("LEFT_EDGE_HOOK_EVIDENCE {result}");
    assert!(observed, "未观察到固定标记注入的钩子事件，边缘结果无效");
}
