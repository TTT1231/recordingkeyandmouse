//! 独立桌面验收：真人点击或快捷键停止，核对原始轨迹和回放事件。
#![cfg(windows)]

use repeat_lib::{
    document::TaskDocument,
    event::{EventKind, MouseButton},
    platform,
    storage::Store,
};
use std::{
    process::Command,
    sync::{Mutex, OnceLock, mpsc},
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::ClientToScreen,
        System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
        UI::WindowsAndMessaging::{
            CallNextHookEx, DispatchMessageW, FindWindowExW, GetMessageW, GetWindowRect,
            GetWindowThreadProcessId, IsWindowVisible, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT,
            PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
            WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_QUIT,
        },
    },
    core::w,
};

#[derive(Clone, Copy)]
struct PhysicalClick {
    msg: u32,
    time: u32,
    x: i32,
    y: i32,
}
static PROBE: OnceLock<Mutex<Vec<PhysicalClick>>> = OnceLock::new();
static PROBE_BOUNDS: OnceLock<RECT> = OnceLock::new();

unsafe extern "system" fn physical_click_probe(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 && matches!(wp.0 as u32, WM_LBUTTONDOWN | WM_LBUTTONUP) {
        let info = unsafe { &*(lp.0 as *const MSLLHOOKSTRUCT) };
        if let Some(bounds) = PROBE_BOUNDS.get()
            && info.flags & LLMHF_INJECTED == 0
            && info.pt.x >= bounds.left
            && info.pt.x < bounds.right
            && info.pt.y >= bounds.top
            && info.pt.y < bounds.bottom
            && let Some(events) = PROBE.get()
        {
            events.lock().unwrap().push(PhysicalClick {
                msg: wp.0 as u32,
                time: info.time,
                x: info.pt.x,
                y: info.pt.y,
            });
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

fn main_for_process(pid: u32) -> Option<HWND> {
    let mut previous = None;
    loop {
        let hwnd = unsafe { FindWindowExW(None, previous, None, w!("Repeat")) }.ok()?;
        let mut owner = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut owner));
        }
        if owner == pid && unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return Some(hwnd);
        }
        previous = Some(hwnd);
    }
}

struct ChildGuard(std::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn overlay_for_process(pid: u32) -> Option<HWND> {
    let mut previous = None;
    loop {
        let hwnd = unsafe { FindWindowExW(None, previous, None, w!("Repeat 执行状态")) }.ok()?;
        let mut owner = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut owner));
        }
        if owner == pid && unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return Some(hwnd);
        }
        previous = Some(hwnd);
    }
}

#[test]
#[ignore = "启动隔离 Repeat，需真人录制普通点击并使用状态条停止"]
fn native_recording_overlay_stop_preserves_prior_click() {
    platform::init_process_dpi();
    let executable = std::env::var_os("REPEAT_E2E_EXECUTABLE")
        .expect("REPEAT_E2E_EXECUTABLE 须指向测试用 Repeat 可执行文件");
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("recording-overlay-e2e-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let doc = TaskDocument::new(
        "状态条录制验收固定样本".into(),
        platform::snapshot_geometry(),
        platform::current_layout_klid(),
    );
    let db = directory.join("tasks.sqlite3");
    let by_hotkey = std::env::var_os("REPEAT_E2E_HOTKEY_STOP").is_some();
    let race = std::env::var_os("REPEAT_E2E_MAIN_RACE").is_some();
    let by_main = race || std::env::var_os("REPEAT_E2E_MAIN_STOP").is_some();
    assert!(!(by_hotkey && by_main), "请只选择一种停止方式");
    let mut store = Store::open(&db).unwrap();
    store.insert(&doc).unwrap();
    store
        .set_setting(
            "hotkeys",
            &serde_json::to_string(&repeat_lib::capture::Hotkeys {
                record_vk: 0x79,
                stop_vk: 0x7a,
            })
            .unwrap(),
        )
        .unwrap();
    let child = Command::new(executable)
        .env("REPEAT_DATA_DIR", &directory)
        .spawn()
        .unwrap();
    let child = ChildGuard(child);
    let instruction = if race {
        "在主窗口空白处点击一次，然后尽快双击主窗口里的停止录制按钮"
    } else if by_hotkey {
        "在主窗口空白处点击一次，移动鼠标到另一处后按 F10 停止"
    } else if by_main {
        "在主窗口空白处点击一次，再点击主窗口里的停止录制按钮"
    } else {
        "在主窗口空白处点击一次，再移动鼠标并点击底部停止"
    };
    println!(
        "RECORDING_OVERLAY_READY pid={} 请开始录制，然后{instruction}。",
        child.0.id()
    );
    let started = Instant::now();
    let hwnd = loop {
        if let Some(hwnd) = overlay_for_process(child.0.id()) {
            break hwnd;
        }
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "未等到录制状态条"
        );
        thread::sleep(Duration::from_millis(50));
    };
    let mut origin = POINT { x: 0, y: 0 };
    assert!(unsafe { ClientToScreen(hwnd, &mut origin) }.as_bool());
    let dpi = platform::snapshot_geometry()
        .monitors
        .iter()
        .find(|monitor| {
            origin.x >= monitor.x
                && origin.y >= monitor.y
                && origin.x < monitor.x + monitor.width
                && origin.y < monitor.y + monitor.height
        })
        .expect("无法确定状态条所在显示器")
        .dpi;
    let scale = f64::from(dpi) / 96.0;
    let bounds = (
        origin.x + (252.0 * scale).round() as i32,
        origin.y + (6.0 * scale).round() as i32,
        origin.x + (292.0 * scale).round() as i32,
        origin.y + (46.0 * scale).round() as i32,
    );
    let mut probe_thread = None;
    if race {
        let main = main_for_process(child.0.id()).expect("无法找到隔离主窗口");
        let mut main_bounds = RECT::default();
        unsafe {
            GetWindowRect(main, &mut main_bounds).unwrap();
        }
        PROBE_BOUNDS.set(main_bounds).unwrap();
        PROBE.get_or_init(|| Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel();
        let worker = thread::spawn(move || unsafe {
            let module = GetModuleHandleW(None).unwrap();
            let hook = SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(physical_click_probe),
                Some(HINSTANCE(module.0)),
                0,
            )
            .unwrap();
            tx.send(GetCurrentThreadId()).unwrap();
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let _ = UnhookWindowsHookEx(hook);
        });
        let id = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("独立点击探针启动失败");
        probe_thread = Some((id, worker));
    }
    println!("RECORDING_OVERLAY_VISIBLE bounds={bounds:?}: 请{instruction}。");
    let wait = Instant::now();
    while overlay_for_process(child.0.id()).is_some() {
        assert!(wait.elapsed() < Duration::from_secs(120), "未等到录制停止");
        thread::sleep(Duration::from_millis(50));
    }
    if let Some((id, worker)) = probe_thread {
        thread::sleep(Duration::from_millis(500));
        unsafe {
            PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0)).unwrap();
        }
        worker.join().unwrap();
    }
    let saved = loop {
        let loaded = Store::open(&db).unwrap().load(&doc.id).unwrap();
        if loaded.revision > doc.revision {
            break loaded;
        }
        assert!(
            wait.elapsed() < Duration::from_secs(120),
            "未观察到隔离录制保存"
        );
        thread::sleep(Duration::from_millis(50));
    };
    let compiled = saved.compile();
    let raw = saved.raw.unwrap();
    if race {
        let physical = PROBE.get().unwrap().lock().unwrap().clone();
        let pairs: Vec<_> = physical
            .windows(2)
            .filter(|pair| pair[0].msg == WM_LBUTTONDOWN && pair[1].msg == WM_LBUTTONUP)
            .collect();
        let in_raw = |event: PhysicalClick| {
            raw.events.iter().any(|recorded| {
                recorded.native_time_ms == event.time
                    && match recorded.kind {
                        EventKind::MouseDown {
                            button: MouseButton::Left,
                            x,
                            y,
                        } if event.msg == WM_LBUTTONDOWN => x == event.x && y == event.y,
                        EventKind::MouseUp {
                            button: MouseButton::Left,
                            x,
                            y,
                        } if event.msg == WM_LBUTTONUP => x == event.x && y == event.y,
                        _ => false,
                    }
            })
        };
        let selected = if pairs.len() == 3 {
            Some((pairs[1], pairs[2]))
        } else {
            None
        };
        let valid = selected.is_some_and(|(first, second)| {
            (first[0].x - second[0].x).abs() <= 12
                && (first[0].y - second[0].y).abs() <= 12
                && !(first[0].x >= bounds.0
                    && first[0].x < bounds.2
                    && first[0].y >= bounds.1
                    && first[0].y < bounds.3)
                && first[0].time != second[0].time
                && first[1].time != second[1].time
        });
        let evidence = serde_json::json!({"fixture":"isolated Tauri; physical probe only inside test window", "physicalEvents":physical.iter().map(|e| serde_json::json!({"msg":if e.msg == WM_LBUTTONDOWN {"down"} else {"up"}, "nativeTimeMs":e.time, "x":e.x, "y":e.y})).collect::<Vec<_>>(), "recordedClicks":raw.events.iter().filter_map(|event| match event.kind { EventKind::MouseDown {button:MouseButton::Left,x,y} => Some(serde_json::json!({"msg":"down","nativeTimeMs":event.native_time_ms,"x":x,"y":y})), EventKind::MouseUp {button:MouseButton::Left,x,y} => Some(serde_json::json!({"msg":"up","nativeTimeMs":event.native_time_ms,"x":x,"y":y})), _=>None }).collect::<Vec<_>>(), "validDoubleClick":valid});
        std::fs::write(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target/recording-main-race-evidence.json"),
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();
        println!("RECORDING_MAIN_RACE_EVIDENCE {evidence}");
        assert!(valid, "INCONCLUSIVE：没有观察到有效的连续两次停止位置点击");
        assert!(
            repeat_lib::document::validate_balanced(&compiled).is_ok(),
            "确认异常：回放输入中出现未配对的按下或抬起: {evidence}"
        );
        let (first, second) = selected.unwrap();
        let first_seen = first.iter().filter(|event| in_raw(**event)).count();
        let second_seen = second.iter().filter(|event| in_raw(**event)).count();
        assert_eq!(first_seen, 0, "确认异常：停止点击保留在录制中");
        assert_eq!(
            second_seen, 2,
            "INCONCLUSIVE：停止后第二次点击没有完整进入录制，无法判断竞态是否命中"
        );
        return;
    }
    let clicks: Vec<_> = raw
        .events
        .iter()
        .filter_map(|event| {
            let (kind, x, y) = match event.kind {
                EventKind::MouseDown {
                    button: MouseButton::Left,
                    x,
                    y,
                } => ("down", x, y),
                EventKind::MouseUp {
                    button: MouseButton::Left,
                    x,
                    y,
                } => ("up", x, y),
                _ => return None,
            };
            Some((event.seq, event.time_us, kind, x, y))
        })
        .collect();
    let in_stop = |x: i32, y: i32| x >= bounds.0 && x < bounds.2 && y >= bounds.1 && y < bounds.3;
    let last_click_up = clicks.iter().rev().find(|(_, _, kind, _, _)| *kind == "up");
    let raw_trailing_moves = last_click_up.map_or(0, |(seq, _, _, _, _)| {
        raw.events
            .iter()
            .filter(|event| event.seq > *seq && matches!(event.kind, EventKind::MouseMove { .. }))
            .count()
    });
    let compiled_last_up = compiled.iter().rev().find(|event| {
        matches!(
            event.kind,
            EventKind::MouseUp {
                button: MouseButton::Left,
                ..
            }
        )
    });
    let compiled_trailing_moves = compiled_last_up.map_or(0, |last_up| {
        compiled
            .iter()
            .filter(|event| {
                event.seq > last_up.seq && matches!(event.kind, EventKind::MouseMove { .. })
            })
            .count()
    });
    let evidence = serde_json::json!({"fixture":"isolated Tauri recording; physical prior click and stop", "stopMethod": if by_hotkey { "hotkey" } else if by_main { "main" } else { "overlay" }, "stopBounds":bounds, "leftButtonEvents":clicks.iter().map(|(seq,time,kind,x,y)| serde_json::json!({"seq":seq,"timeUs":time,"kind":kind,"x":x,"y":y})).collect::<Vec<_>>(), "rawMovesAfterLastClick": raw_trailing_moves, "compiledMovesAfterLastClick": compiled_trailing_moves});
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(if by_hotkey {
            "target/recording-hotkey-evidence.json"
        } else if by_main {
            "target/recording-main-stop-evidence.json"
        } else {
            "target/recording-overlay-evidence.json"
        }),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
    assert!(
        !clicks.iter().any(|(_, _, _, x, y)| in_stop(*x, *y)),
        "停止点击不应写入录制: {evidence}"
    );
    assert!(
        clicks
            .windows(2)
            .any(|pair| pair[0].2 == "down" && pair[1].2 == "up"),
        "应保留前一次完整点击: {evidence}"
    );
    assert!(
        raw_trailing_moves > 0,
        "应保留真实停止路径的原始证据: {evidence}"
    );
    assert_eq!(
        compiled_trailing_moves, raw_trailing_moves,
        "停止前的鼠标移动应保留在回放中: {evidence}"
    );
    println!("RECORDING_OVERLAY_EVIDENCE {evidence}");
}
