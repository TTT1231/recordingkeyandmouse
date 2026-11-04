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
            CallNextHookEx, DispatchMessageW, FindWindowExW, GetClientRect, GetMessageW,
            GetWindowRect, GetWindowThreadProcessId, IsWindowVisible, LLMHF_INJECTED, MSG,
            MSLLHOOKSTRUCT, PostThreadMessageW, SetWindowsHookExW, TranslateMessage,
            UnhookWindowsHookEx, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_QUIT,
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

fn assert_status_has_no_rectangular_frame(hwnd: HWND) -> serde_json::Value {
    let (mut outer, mut client, mut origin) = (RECT::default(), RECT::default(), POINT::default());
    unsafe {
        GetWindowRect(hwnd, &mut outer).unwrap();
        GetClientRect(hwnd, &mut client).unwrap();
        assert!(ClientToScreen(hwnd, &mut origin).as_bool());
    }
    assert_eq!(
        (outer.left, outer.top),
        (origin.x, origin.y),
        "状态条外不能有系统边框偏移"
    );
    assert_eq!(
        (outer.right - outer.left, outer.bottom - outer.top),
        (client.right, client.bottom),
        "透明圆角外不能残留矩形非客户区"
    );
    serde_json::json!({"outerWidth":outer.right-outer.left,"outerHeight":outer.bottom-outer.top,"clientWidth":client.right,"clientHeight":client.bottom,"nonClientOffsetX":origin.x-outer.left,"nonClientOffsetY":origin.y-outer.top})
}

/// A full Tauri recording uses its global hotkey, native preparation hooks and
/// persisted document. Synthetic keys are a fixed fixture, not physical-input proof.
#[test]
#[ignore = "启动隔离 Repeat 并发送 F10/F6/F7 固定样本，核对准备取消与事件起点"]
fn native_recording_preparation_excludes_early_input() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY,
    };
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

    fn wait_until(label: &str, mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !ready() {
            assert!(Instant::now() < deadline, "等待超时：{label}");
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn tap(vk: u16) {
        let inputs = [false, true].map(|up| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        Default::default()
                    },
                    dwExtraInfo: 0x50524550,
                    ..Default::default()
                },
            },
        });
        assert_eq!(
            unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) },
            2,
            "固定样本输入失败"
        );
    }
    fn border_visible(pid: u32) -> bool {
        let mut previous = None;
        while let Ok(hwnd) =
            unsafe { FindWindowExW(None, previous, None, w!("Repeat 执行范围")) }
        {
            let mut owner = 0;
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner)) };
            if owner == pid && unsafe { IsWindowVisible(hwnd) }.as_bool() {
                return true;
            }
            previous = Some(hwnd);
        }
        false
    }

    platform::init_process_dpi();
    let executable = std::env::var_os("REPEAT_E2E_EXECUTABLE")
        .expect("REPEAT_E2E_EXECUTABLE 须指向测试用 Repeat 可执行文件");
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!(
            "recording-preparation-e2e-{}",
            repeat_lib::document::new_id()
        ));
    std::fs::create_dir(&directory).unwrap();
    let mut store = Store::open(&directory.join("tasks.sqlite3")).unwrap();
    let mut doc = TaskDocument::new(
        "录制准备固定样本".into(),
        platform::snapshot_geometry(),
        platform::current_layout_klid(),
    );
    doc.replace_recording(repeat_lib::document::RawRecording {
        gestures: Default::default(),
        started_at_wall_ms: 0,
        duration_us: 100_000,
        events: [
            EventKind::KeyDown {
                vk: 0x74,
                scan: 0x3f,
                extended: false,
            },
            EventKind::KeyUp {
                vk: 0x74,
                scan: 0x3f,
                extended: false,
            },
        ]
        .into_iter()
        .enumerate()
        .map(|(index, kind)| repeat_lib::event::RecordedEvent {
            seq: index as u64 + 1,
            time_us: index as i64 * 50_000,
            native_time_ms: index as u32 * 50,
            kind,
        })
        .collect(),
        geometry: doc.geometry.clone(),
        layout_klid: doc.layout_klid.clone(),
    });
    store.insert(&doc).unwrap();
    store
        .set_setting("hotkeys", r#"{"recordVk":121,"stopVk":122}"#)
        .unwrap();
    let original = serde_json::to_value(&doc).unwrap();
    let child = ChildGuard(
        Command::new(executable)
            .env("REPEAT_DATA_DIR", &directory)
            .spawn()
            .unwrap(),
    );
    let pid = child.0.id();
    wait_until("隔离主窗口", || main_for_process(pid).is_some());
    // Overlay creation follows hotkey initialization. Wait for its hidden HWND,
    // then give the WebView enough time to mount before the fixed input sample.
    wait_until("状态条初始化", || {
        let mut previous = None;
        while let Ok(hwnd) =
            unsafe { FindWindowExW(None, previous, None, w!("Repeat 执行状态")) }
        {
            let mut owner = 0;
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner)) };
            if owner == pid {
                return true;
            }
            previous = Some(hwnd);
        }
        false
    });
    unsafe {
        let _ = SetForegroundWindow(main_for_process(pid).unwrap());
    }
    thread::sleep(Duration::from_secs(2));
    tap(0x79);
    wait_until("准备状态条", || overlay_for_process(pid).is_some());
    let preparation_window =
        assert_status_has_no_rectangular_frame(overlay_for_process(pid).unwrap());
    assert!(!border_visible(pid), "准备期间不应显示录制边框");
    tap(0x75); // F6 during preparation must not enter a recording.
    tap(0x79); // The record hotkey cancels this preparation.
    wait_until("取消准备", || overlay_for_process(pid).is_none());
    thread::sleep(Duration::from_millis(3200));
    assert!(overlay_for_process(pid).is_none(), "取消后不应延迟开始录制");
    assert_eq!(
        serde_json::to_value(store.load(&doc.id).unwrap()).unwrap(),
        original
    );

    tap(0x79);
    wait_until("无输入录制开始", || border_visible(pid));
    thread::sleep(Duration::from_millis(200));
    tap(0x79);
    wait_until("无输入录制结束", || {
        overlay_for_process(pid).is_none()
    });

    let requested_at_ms = repeat_lib::document::wall_ms();
    tap(0x79);
    wait_until("再次准备", || overlay_for_process(pid).is_some());
    assert_eq!(
        store.load(&doc.id).unwrap().revision,
        doc.revision,
        "只有录制控制键的样本不得替换原任务"
    );
    tap(0x75);
    wait_until("正式录制边框", || border_visible(pid));
    let recording_window =
        assert_status_has_no_rectangular_frame(overlay_for_process(pid).unwrap());
    thread::sleep(Duration::from_millis(200));
    tap(0x76); // F7 after preparation must be captured as a complete pair.
    thread::sleep(Duration::from_millis(150));
    tap(0x79);
    wait_until("录制保存", || {
        store.load(&doc.id).unwrap().revision > doc.revision
    });
    let saved = store.load(&doc.id).unwrap();
    let raw = saved.raw.as_ref().expect("缺少原始录制");
    let keys: Vec<_> = raw
        .events
        .iter()
        .filter(|e| e.kind.vk().is_some())
        .collect();
    assert_eq!(keys.len(), 2, "仅正式录制阶段的 F7 按下与抬起应被保存");
    assert!(matches!(keys[0].kind, EventKind::KeyDown { vk: 0x76, .. }));
    assert!(matches!(keys[1].kind, EventKind::KeyUp { vk: 0x76, .. }));
    assert!(
        raw.started_at_wall_ms - requested_at_ms >= 3000,
        "录制提前开始"
    );
    assert!(keys[0].time_us >= 0 && keys[1].time_us >= keys[0].time_us);
    assert!(raw.duration_us > keys[1].time_us, "结尾停顿应保留");
    assert!(repeat_lib::document::validate_balanced(&saved.compile()).is_ok());
    let screen = saved
        .screen_recording
        .as_ref()
        .expect("录制须保存桌面画面引用");
    screen.validate(saved.duration_us).unwrap();
    let video = directory
        .join("recordings")
        .join(format!("{}.mp4", screen.id));
    assert!(std::fs::metadata(&video).unwrap().len() > 1000);
    assert_eq!(
        std::fs::read_dir(directory.join("recordings"))
            .unwrap()
            .count(),
        1,
        "只有成功录制应留下录像文件"
    );
    let reopened = Store::open(&directory.join("tasks.sqlite3")).unwrap();
    assert_eq!(
        reopened.load(&doc.id).unwrap().screen_recording.as_ref(),
        Some(screen)
    );
    let evidence = serde_json::json!({
        "fixture": "isolated Tauri; synthetic F10 controls, F6 preparation, F7 recording",
        "preparationWindow": preparation_window,
        "recordingWindow": recording_window,
        "geometry": raw.geometry,
        "cancelPreservedDocument": true,
        "emptyCapturePreservedDocument": true,
        "preparationMs": raw.started_at_wall_ms - requested_at_ms,
        "durationUs": raw.duration_us,
        "recordedKeys": keys,
        "earlyInputExcluded": true,
        "physicalInputVerified": false,
        "videoPath": video,
        "screenRecording": screen,
    });
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/recording-preparation-evidence.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
    println!("RECORDING_PREPARATION_EVIDENCE {evidence}");
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
