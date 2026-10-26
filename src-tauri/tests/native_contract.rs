//! Windows 实机契约测试。会向专用接收窗口发送合成输入；显式 --ignored 才运行。
//! 测试数据均为固定样本，不读取、输出用户输入。长样本可设 REPEAT_NATIVE_SECONDS=180。
#![cfg(windows)]
use repeat_lib::{
    capture::{Hotkeys, RecordingBuffer},
    document::{EditRequest, RawRecording, TaskDocument},
    event::{EventKind, MouseButton, RecordedEvent, WheelAxis},
    geometry::to_normalized_absolute,
    platform,
    scheduler::{
        LATE_THRESHOLD, ReplayDriver, ScheduleSpec, StopSignal, compute_stats, run_replay,
    },
    storage::Store,
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::w,
};
#[derive(Clone, Debug)]
struct Received {
    msg: u32,
    value: usize,
    x: i32,
    y: i32,
    at: Instant,
}
type Messages = Arc<Mutex<Vec<Received>>>;
thread_local! { static MESSAGES:RefCell<Option<Messages>>=const {RefCell::new(None)}; }
unsafe extern "system" fn receiver_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if matches!(
        msg,
        WM_KEYDOWN
            | WM_KEYUP
            | WM_SYSKEYDOWN
            | WM_SYSKEYUP
            | WM_CHAR
            | WM_LBUTTONDOWN
            | WM_LBUTTONUP
            | WM_RBUTTONDOWN
            | WM_RBUTTONUP
            | WM_MBUTTONDOWN
            | WM_MBUTTONUP
            | WM_MOUSEWHEEL
            | WM_MOUSEHWHEEL
    ) {
        let r = Received {
            msg,
            value: wp.0,
            x: (lp.0 as u16 as i16) as i32,
            y: ((lp.0 >> 16) as u16 as i16) as i32,
            at: Instant::now(),
        };
        MESSAGES.with(|m| {
            if let Some(messages) = m.borrow().as_ref() {
                messages.lock().unwrap().push(r);
            }
        });
    }
    if msg == WM_APP + 12 {
        unsafe {
            let foreground_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
            let own = windows::Win32::System::Threading::GetCurrentThreadId();
            let attached =
                windows::Win32::System::Threading::AttachThreadInput(own, foreground_thread, true)
                    .as_bool();
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
            if attached {
                let _ = windows::Win32::System::Threading::AttachThreadInput(
                    own,
                    foreground_thread,
                    false,
                );
            }
        }
        return LRESULT(0);
    }
    if msg == WM_APP + 11 {
        unsafe {
            let child = CreateWindowExW(
                Default::default(),
                w!("EDIT"),
                w!(""),
                WS_CHILD
                    | WS_VISIBLE
                    | WS_BORDER
                    | WINDOW_STYLE(ES_MULTILINE as u32 | ES_WANTRETURN as u32),
                10,
                10,
                700,
                120,
                Some(hwnd),
                None,
                Some(GetModuleHandleW(None).unwrap().into()),
                None,
            )
            .unwrap();
            let _ = SetFocus(Some(child));
            return LRESULT(child.0 as isize);
        }
    }
    if msg == WM_CLOSE {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return LRESULT(0);
    }
    if msg == WM_DESTROY {
        unsafe {
            PostQuitMessage(0);
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}
struct Receiver {
    hwnd: usize,
    messages: Messages,
    join: Option<thread::JoinHandle<()>>,
    origin: (i32, i32),
}
impl Receiver {
    fn new(require_focus: bool) -> Self {
        platform::init_process_dpi();
        let messages = Arc::new(Mutex::new(Vec::new()));
        let copy = messages.clone();
        let (tx, rx) = mpsc::channel();
        let join = thread::spawn(move || unsafe {
            let english = LoadKeyboardLayoutW(w!("00000409"), KLF_ACTIVATE).unwrap();
            let instance: HINSTANCE = GetModuleHandleW(None).unwrap().into();
            let class = WNDCLASSW {
                lpfnWndProc: Some(receiver_proc),
                hInstance: instance,
                lpszClassName: w!("RepeatContractReceiver"),
                ..Default::default()
            };
            assert_ne!(RegisterClassW(&class), 0);
            let hwnd = CreateWindowExW(
                Default::default(),
                w!("RepeatContractReceiver"),
                w!("Repeat 原生验证接收器（固定测试输入）"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                200,
                150,
                900,
                650,
                None,
                None,
                Some(instance),
                None,
            )
            .unwrap();
            MESSAGES.with(|m| *m.borrow_mut() = Some(copy));
            let foreground_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
            let own_thread = windows::Win32::System::Threading::GetCurrentThreadId();
            let attached = windows::Win32::System::Threading::AttachThreadInput(
                own_thread,
                foreground_thread,
                true,
            )
            .as_bool();
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
            let _ = ActivateKeyboardLayout(english, Default::default());
            if attached {
                let _ = windows::Win32::System::Threading::AttachThreadInput(
                    own_thread,
                    foreground_thread,
                    false,
                );
            }
            let mut pt = POINT::default();
            let _ = windows::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut pt);
            tx.send((hwnd.0 as usize, (pt.x, pt.y))).unwrap();
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            MESSAGES.with(|m| *m.borrow_mut() = None);
            let _ = UnregisterClassW(w!("RepeatContractReceiver"), Some(instance));
        });
        let (hwnd, origin) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        thread::sleep(Duration::from_millis(150));
        let receiver = Self {
            hwnd,
            messages,
            join: Some(join),
            origin,
        };
        if require_focus {
            assert_eq!(
                unsafe { GetForegroundWindow() }.0 as usize,
                hwnd,
                "测试接收器没有获得焦点，未发送输入"
            );
        }
        receiver
    }

    fn clear(&self) {
        thread::sleep(Duration::from_millis(100));
        self.messages.lock().unwrap().clear();
    }
    fn messages(&self) -> Vec<Received> {
        thread::sleep(Duration::from_millis(150));
        self.messages.lock().unwrap().clone()
    }
}
impl Drop for Receiver {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(self.hwnd as *mut _)),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            );
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
fn key(vk: u32, down: bool) -> EventKind {
    if down {
        EventKind::KeyDown {
            vk,
            scan: 0,
            extended: false,
        }
    } else {
        EventKind::KeyUp {
            vk,
            scan: 0,
            extended: false,
        }
    }
}
fn external(kind: EventKind) {
    external_at(kind, 0);
}
fn external_at(kind: EventKind, native_time: u32) {
    let geo = platform::snapshot_geometry();
    let virt = geo.virtual_screen().unwrap();
    let mut input = match kind {
        EventKind::KeyDown { vk, .. } | EventKind::KeyUp { vk, .. } => INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk as u16),
                    dwFlags: if kind.is_key_up() {
                        KEYEVENTF_KEYUP
                    } else {
                        Default::default()
                    },
                    dwExtraInfo: 0x54455354,
                    ..Default::default()
                },
            },
        },
        EventKind::Unicode { unit, up } => INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wScan: unit,
                    dwFlags: KEYBD_EVENT_FLAGS(
                        KEYEVENTF_UNICODE.0 | if up { KEYEVENTF_KEYUP.0 } else { 0 },
                    ),
                    dwExtraInfo: 0x54455354,
                    ..Default::default()
                },
            },
        },
        _ => {
            let (x, y) = kind.position().unwrap();
            let (nx, ny) = to_normalized_absolute(x, y, &virt);
            let (flag, data) = match kind {
                EventKind::MouseMove { .. } => (MOUSE_EVENT_FLAGS(0), 0),
                EventKind::MouseDown { button, .. } => (
                    match button {
                        MouseButton::Left => MOUSEEVENTF_LEFTDOWN,
                        MouseButton::Right => MOUSEEVENTF_RIGHTDOWN,
                        MouseButton::Middle => MOUSEEVENTF_MIDDLEDOWN,
                        _ => panic!("not in fixture"),
                    },
                    0,
                ),
                EventKind::MouseUp { button, .. } => (
                    match button {
                        MouseButton::Left => MOUSEEVENTF_LEFTUP,
                        MouseButton::Right => MOUSEEVENTF_RIGHTUP,
                        MouseButton::Middle => MOUSEEVENTF_MIDDLEUP,
                        _ => panic!("not in fixture"),
                    },
                    0,
                ),
                EventKind::Wheel { axis, delta, .. } => (
                    if axis == WheelAxis::Vertical {
                        MOUSEEVENTF_WHEEL
                    } else {
                        MOUSEEVENTF_HWHEEL
                    },
                    delta as u32,
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
                            flag.0
                                | MOUSEEVENTF_MOVE.0
                                | MOUSEEVENTF_ABSOLUTE.0
                                | MOUSEEVENTF_VIRTUALDESK.0,
                        ),
                        dwExtraInfo: 0x54455354,
                        ..Default::default()
                    },
                },
            }
        }
    };
    if input.r#type == INPUT_KEYBOARD {
        input.Anonymous.ki.time = native_time;
    } else {
        input.Anonymous.mi.time = native_time;
    }
    assert_eq!(
        unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) },
        1,
        "测试外部输入失败"
    );
}
#[derive(Default)]
struct ExternalHeld {
    keys: std::collections::BTreeSet<u32>,
    buttons: BTreeMap<MouseButton, (i32, i32)>,
}
impl ExternalHeld {
    fn note(&mut self, event: EventKind) {
        match event {
            EventKind::KeyDown { vk, .. } => {
                self.keys.insert(vk);
            }
            EventKind::KeyUp { vk, .. } => {
                self.keys.remove(&vk);
            }
            EventKind::MouseDown { button, x, y } => {
                self.buttons.insert(button, (x, y));
            }
            EventKind::MouseUp { button, .. } => {
                self.buttons.remove(&button);
            }
            _ => {}
        }
    }
}
impl Drop for ExternalHeld {
    fn drop(&mut self) {
        for &vk in &self.keys {
            external(key(vk, false));
        }
        for (&button, &(x, y)) in &self.buttons {
            external(EventKind::MouseUp { button, x, y });
        }
    }
}
fn toggle_app() {
    external(key(0x79, true));
    external(key(0x79, false));
    thread::sleep(Duration::from_millis(350));
}
struct AppCaptureGuard(bool);
impl Drop for AppCaptureGuard {
    fn drop(&mut self) {
        if self.0 {
            toggle_app();
        }
    }
}
fn fixture(origin: (i32, i32)) -> Vec<(u64, EventKind)> {
    let (x, y) = (origin.0 + 180, origin.1 + 180);
    let mut items = vec![(200_000, EventKind::MouseMove { x, y })];
    for i in 0..1000 {
        items.push((
            300_000 + i * 2000,
            EventKind::MouseMove {
                x: x + (i % 250) as i32,
                y: y + ((i / 250) % 2) as i32 * 20,
            },
        ));
    }
    let mut at = 2_600_000;
    for button in [
        MouseButton::Left,
        MouseButton::Right,
        MouseButton::Middle,
        MouseButton::Left,
        MouseButton::Left,
    ] {
        items.push((at, EventKind::MouseDown { button, x, y }));
        items.push((at + 25_000, EventKind::MouseUp { button, x, y }));
        at += 100_000;
    }
    items.extend([
        (
            3_300_000,
            EventKind::Wheel {
                axis: WheelAxis::Vertical,
                delta: 120,
                x,
                y,
            },
        ),
        (
            3_400_000,
            EventKind::Wheel {
                axis: WheelAxis::Vertical,
                delta: -240,
                x,
                y,
            },
        ),
        (
            3_500_000,
            EventKind::Wheel {
                axis: WheelAxis::Horizontal,
                delta: 120,
                x,
                y,
            },
        ),
        (3_700_000, key(0x41, true)),
        (3_730_000, key(0x41, true)),
        (3_760_000, key(0x41, true)),
        (3_800_000, key(0x41, false)),
        (3_900_000, key(0x09, true)),
        (3_930_000, key(0x09, false)),
        (4_000_000, key(0xA2, true)),
        (4_020_000, key(0x42, true)),
        (4_050_000, key(0x42, false)),
        (4_080_000, key(0xA2, false)),
        (
            4_200_000,
            EventKind::MouseDown {
                button: MouseButton::Left,
                x,
                y,
            },
        ),
        (4_220_000, key(0xA0, true)),
        (
            4_240_000,
            EventKind::MouseMove {
                x: x + 80,
                y: y + 40,
            },
        ),
        (4_260_000, key(0xA0, false)),
        (
            4_300_000,
            EventKind::MouseUp {
                button: MouseButton::Left,
                x: x + 80,
                y: y + 40,
            },
        ),
    ]);
    items
}
fn semantic(kind: EventKind) -> Option<String> {
    match kind {
        EventKind::MouseMove { .. } => None,
        EventKind::KeyDown { vk, .. } => Some(format!("down:{vk}")),
        EventKind::KeyUp { vk, .. } => Some(format!("up:{vk}")),
        EventKind::MouseDown { button, .. } => Some(format!("mouse-down:{button:?}")),
        EventKind::MouseUp { button, .. } => Some(format!("mouse-up:{button:?}")),
        EventKind::Wheel { axis, delta, .. } => Some(format!("wheel:{axis:?}:{delta}")),
        EventKind::Unicode { unit, up } => Some(format!("unicode:{unit}:{up}")),
    }
}
struct FixtureDriver {
    inner: platform::windows::driver::NativeDriver,
    target: usize,
}
impl ReplayDriver for FixtureDriver {
    fn now(&mut self) -> Instant {
        self.inner.now()
    }
    fn wait_until(&mut self, t: Instant) -> repeat_lib::scheduler::PacerOutcome {
        self.inner.wait_until(t)
    }
    fn inject(&mut self, kind: &EventKind) -> Result<(), String> {
        if !matches!(kind, EventKind::MouseMove { .. })
            && unsafe { GetForegroundWindow() }.0 as usize != self.target
        {
            return Err("测试接收窗口失焦，未发送输入".into());
        }
        self.inner.inject(kind)
    }
    fn release_held(&mut self) -> Result<Vec<String>, String> {
        self.inner.release_held()
    }
}
fn counts(messages: &[Received]) -> BTreeMap<u32, usize> {
    let mut result = BTreeMap::new();
    for m in messages {
        *result.entry(m.msg).or_default() += 1;
    }
    result
}
#[test]
#[ignore = "向专用接收窗口发送固定输入，需交互桌面；可选同时验证隔离的 Repeat 测试实例"]
fn native_capture_edit_save_replay_contract() {
    let seconds = std::env::var("REPEAT_NATIVE_SECONDS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(6)
        .max(6);
    let receiver = Receiver::new(seconds < 20);
    let stop = StopSignal::new();
    let service = platform::start_input_service(Arc::new(|_| {})).unwrap();
    service
        .set_hotkeys(Hotkeys {
            record_vk: 0x75,
            stop_vk: 0x76,
        })
        .unwrap();
    service.ensure_stop_available().unwrap();
    let app_db = std::env::var_os("REPEAT_VERIFY_APP_DB");
    let mut app_capture = AppCaptureGuard(app_db.is_some());
    if app_capture.0 {
        toggle_app();
    }
    receiver.clear();
    let buffer = Arc::new(Mutex::new(RecordingBuffer::new(vec![0x75, 0x76])));
    buffer.lock().unwrap().begin();
    service.start_capture(buffer.clone()).unwrap();
    let mut items = fixture(receiver.origin);
    if seconds >= 20 {
        items.retain(|(_, kind)| matches!(kind, EventKind::MouseMove { .. }));
    }
    if seconds >= 20 {
        let (x, y) = (receiver.origin.0 + 200, receiver.origin.1 + 200);
        let mid = seconds * 500_000;
        let end = seconds * 1_000_000 - 1_000_000;
        items.extend([
            (
                mid,
                EventKind::MouseMove {
                    x: x + 20,
                    y: y + 10,
                },
            ),
            (
                end,
                EventKind::MouseMove {
                    x: x + 40,
                    y: y + 20,
                },
            ),
        ]);
    }
    let mut pacer = platform::make_driver(&stop, &platform::snapshot_geometry()).unwrap();
    let began = Instant::now();
    let mut held = ExternalHeld::default();
    for (index, &(us, kind)) in items.iter().enumerate() {
        assert_eq!(
            pacer.wait_until(began + Duration::from_micros(us)),
            repeat_lib::scheduler::PacerOutcome::Deadline
        );
        if !matches!(kind, EventKind::MouseMove { .. }) {
            assert_eq!(
                unsafe { GetForegroundWindow() }.0 as usize,
                receiver.hwnd,
                "焦点改变，停止固定样本"
            );
        }
        external_at(kind, 0x5A000001 + index as u32);
        held.note(kind);
    }
    assert_eq!(
        pacer.wait_until(began + Duration::from_secs(seconds)),
        repeat_lib::scheduler::PacerOutcome::Deadline
    );
    service.stop_capture().unwrap();
    let mut raw = buffer.lock().unwrap().finish();
    assert!(
        raw.events
            .windows(2)
            .all(|w| w[0].seq + 1 == w[1].seq && w[0].time_us <= w[1].time_us)
    );
    raw.events.retain(|e| {
        e.native_time_ms > 0x5A000000 && e.native_time_ms <= 0x5A000000 + items.len() as u32
    });
    for (i, event) in raw.events.iter_mut().enumerate() {
        event.seq = i as u64 + 1;
    }
    let source_received: Vec<_> = receiver
        .messages()
        .into_iter()
        .filter(|m| m.msg != WM_CHAR)
        .collect();
    if app_capture.0 {
        toggle_app();
        app_capture.0 = false;
    }
    assert!(!raw.overflow);
    let want: Vec<_> = items.iter().filter_map(|(_, k)| semantic(*k)).collect();
    let got: Vec<_> = raw.events.iter().filter_map(|e| semantic(e.kind)).collect();
    assert!(got == want, "原生捕获按下、抬起、重复和滚轮的顺序不一致");
    assert!(raw.events.len() >= 1000);
    for (index, (_, kind)) in items.iter().enumerate() {
        assert!(
            raw.events
                .iter()
                .any(|e| e.native_time_ms == 0x5A000001 + index as u32
                    && match (kind.position(), e.kind.position()) {
                        (Some((x, y)), Some((ex, ey))) =>
                            (x - ex).abs() <= 1 && (y - ey).abs() <= 1,
                        _ => semantic(*kind) == semantic(e.kind),
                    }),
            "已标记的固定输入没有完整捕获"
        );
    }
    assert!(
        raw.events
            .windows(2)
            .all(|w| w[0].seq + 1 == w[1].seq && w[0].time_us <= w[1].time_us)
    );
    assert!(raw.duration_us >= seconds as i64 * 1_000_000);
    let geometry = platform::snapshot_geometry();
    let mut doc = TaskDocument::new(
        "原生契约样本".into(),
        geometry.clone(),
        platform::current_layout_klid(),
    );
    doc.replace_recording(RawRecording {
        gestures: Default::default(),
        started_at_wall_ms: 0,
        duration_us: raw.duration_us,
        events: raw.events.clone(),
        geometry: geometry.clone(),
        layout_klid: platform::current_layout_klid(),
    });
    doc.validate_execution().unwrap();
    if let Some(path) = app_db {
        let store = Store::open(std::path::Path::new(&path)).unwrap();
        let active = store.setting("activeTask").unwrap().unwrap();
        let from_app = store.load(&active).unwrap();
        let app_raw = from_app
            .raw
            .as_ref()
            .expect("Repeat 全局快捷键未能结束并保存录制");
        let app_semantic: Vec<_> = app_raw
            .events
            .iter()
            .filter(|e| {
                e.native_time_ms > 0x5A000000 && e.native_time_ms <= 0x5A000000 + items.len() as u32
            })
            .filter_map(|e| semantic(e.kind))
            .collect();
        assert!(app_semantic == want, "独立 Repeat 进程录制结果不一致");
        assert!(app_raw.events.len() >= 1000);
        println!(
            "APP_CAPTURE duration_us={} raw_events={} actions={}",
            from_app.duration_us,
            app_raw.events.len(),
            from_app.actions.len()
        );
    }
    let first = doc.actions[0].id.clone();
    doc.apply(EditRequest::InsertWait {
        after_action: Some(first),
        duration_us: 1_000_000,
    })
    .unwrap();
    assert_eq!(doc.duration_us, raw.duration_us + 1_000_000);
    doc.validate_execution().unwrap();
    let db_path = std::env::temp_dir().join(format!(
        "repeat-native-{}.sqlite3",
        repeat_lib::document::new_id()
    ));
    {
        let mut store = Store::open(&db_path).unwrap();
        store.insert(&doc).unwrap();
    }
    let reopened = {
        let store = Store::open(&db_path).unwrap();
        store.load(&doc.id).unwrap()
    };
    assert_eq!(
        serde_json::to_string(&doc).unwrap(),
        serde_json::to_string(&reopened).unwrap()
    );
    unsafe {
        let _ = SendMessageW(HWND(receiver.hwnd as *mut _), WM_APP + 12, None, None);
    }
    receiver.clear();
    let replay_stop = StopSignal::new();
    let mut driver = FixtureDriver {
        inner: platform::make_driver(&replay_stop, &geometry).unwrap(),
        target: receiver.hwnd,
    };
    let spec = ScheduleSpec {
        events: Arc::new(reopened.compile()),
        duration_us: reopened.duration_us,
        rounds: 1,
    };
    let outcome = run_replay(&spec, &mut driver, &replay_stop, LATE_THRESHOLD, |_| {});
    let replay_received: Vec<_> = receiver
        .messages()
        .into_iter()
        .filter(|m| m.msg != WM_CHAR)
        .collect();
    assert!(outcome.failure.is_none(), "{:?}", outcome.failure);
    assert_eq!(outcome.rounds_completed, 1);
    assert_eq!(outcome.events_injected, raw.events.len() as u64);
    assert_eq!(
        counts(&source_received),
        counts(&replay_received),
        "接收窗口的按键、点击、滚轮消息数量不同"
    );
    let source_order: Vec<_> = source_received.iter().map(|m| (m.msg, m.value)).collect();
    let replay_order: Vec<_> = replay_received.iter().map(|m| (m.msg, m.value)).collect();
    assert!(source_order == replay_order, "接收窗口的事件顺序不同");
    for message in &replay_received {
        if matches!(
            message.msg,
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN
        ) {
            assert!((message.x - 180).abs() <= 1 || (message.x - 200).abs() <= 1);
            assert!((message.y - 180).abs() <= 1 || (message.y - 200).abs() <= 1);
        }
    }
    let stats = compute_stats(&outcome.timing);
    let reception_span = match (replay_received.first(), replay_received.last()) {
        (Some(a), Some(b)) => b.at.duration_since(a.at).as_micros(),
        _ => 0,
    };
    println!(
        "NATIVE_CONTRACT seconds={seconds} raw_events={} actions={} receiver_events={} replay_p95_us={} max_us={} drift_us={} reception_span_us={reception_span}",
        raw.events.len(),
        reopened.actions.len(),
        replay_received.len(),
        stats.p95_abs_us,
        stats.max_abs_us,
        stats.drift_us
    );
    let evidence = serde_json::json!({"durationUs":raw.duration_us,"rawEvents":raw.events.len(),"actions":reopened.actions.len(),"sourceMessageCounts":counts(&source_received),"replayMessageCounts":counts(&replay_received),"stats":stats,"timing":outcome.timing});
    let evidence_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/native-contract-evidence.json");
    std::fs::write(evidence_path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    service.shutdown();
    let _ = std::fs::remove_file(db_path);
}
#[test]
#[ignore = "真实文本与停止按键收尾；向专用接收窗口发送固定输入"]
fn native_unicode_global_stop_and_cleanup() {
    let receiver = Receiver::new(true);
    let stop = StopSignal::new();
    let copy = stop.clone();
    let toggles = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let count = toggles.clone();
    let service = platform::start_input_service(Arc::new(move |action| {
        if action == platform::HotkeyAction::StopExecution {
            copy.signal();
        }
        if action == platform::HotkeyAction::ToggleRecording {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }))
    .unwrap();
    service
        .set_hotkeys(Hotkeys {
            record_vk: 0x75,
            stop_vk: 0x76,
        })
        .unwrap();
    let mut doc = TaskDocument::new(
        "文本".into(),
        platform::snapshot_geometry(),
        platform::current_layout_klid(),
    );
    doc.apply(EditRequest::InsertText {
        after_action: None,
        text: "中文 A😀!\n下一行".into(),
    })
    .unwrap();
    receiver.clear();
    let mut driver = platform::make_driver(&stop, &doc.geometry).unwrap();
    let spec = ScheduleSpec {
        events: Arc::new(doc.compile()),
        duration_us: 0,
        rounds: 3,
    };
    let outcome = run_replay(&spec, &mut driver, &stop, LATE_THRESHOLD, |_| {});
    assert!(outcome.failure.is_none());
    assert_eq!(outcome.rounds_completed, 3);
    let chars: Vec<_> = receiver
        .messages()
        .iter()
        .filter(|m| m.msg == WM_CHAR)
        .map(|m| m.value as u16)
        .collect();
    let text = String::from_utf16(&chars).unwrap().replace('\r', "\n");
    assert_eq!(text, "中文 A😀!\n下一行".repeat(3));
    let edit = unsafe {
        HWND(SendMessageW(HWND(receiver.hwnd as *mut _), WM_APP + 11, None, None).0 as *mut _)
    };
    let edit_stop = StopSignal::new();
    let mut edit_driver = platform::make_driver(&edit_stop, &doc.geometry).unwrap();
    let edit_outcome = run_replay(&spec, &mut edit_driver, &edit_stop, LATE_THRESHOLD, |_| {});
    assert!(edit_outcome.failure.is_none());
    thread::sleep(Duration::from_millis(150));
    let mut text_buffer = vec![0u16; 1024];
    let text_len = unsafe { GetWindowTextW(edit, &mut text_buffer) };
    let entered = String::from_utf16(&text_buffer[..text_len as usize])
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(
        entered,
        "中文 A😀!\n下一行".repeat(3),
        "标准 Windows 多行输入框应收到中文、符号和换行"
    );
    receiver.clear();
    external(key(0x75, true));
    external(key(0x75, false));
    thread::sleep(Duration::from_millis(80));
    assert_eq!(
        toggles.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "失焦全局录制快捷键应触发"
    );
    let (x, y) = (receiver.origin.0 + 180, receiver.origin.1 + 180);
    let events = vec![
        RecordedEvent {
            seq: 1,
            time_us: 0,
            native_time_ms: 0,
            kind: key(0xA2, true),
        },
        RecordedEvent {
            seq: 2,
            time_us: 50_000,
            native_time_ms: 0,
            kind: EventKind::MouseDown {
                button: MouseButton::Left,
                x,
                y,
            },
        },
        RecordedEvent {
            seq: 3,
            time_us: 5_000_000,
            native_time_ms: 0,
            kind: key(0xA2, false),
        },
        RecordedEvent {
            seq: 4,
            time_us: 5_050_000,
            native_time_ms: 0,
            kind: EventKind::MouseUp {
                button: MouseButton::Left,
                x,
                y,
            },
        },
    ];
    service.arm_execution(true).unwrap();
    let stopper = thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        external(EventKind::MouseMove {
            x: x + 70,
            y: y + 45,
        });
        external(key(0x76, true));
        external(key(0x76, false));
    });
    let mut driver = platform::make_driver(&stop, &doc.geometry).unwrap();
    let outcome = run_replay(
        &ScheduleSpec {
            events: Arc::new(events),
            duration_us: 6_000_000,
            rounds: 3,
        },
        &mut driver,
        &stop,
        LATE_THRESHOLD,
        |_| {},
    );
    stopper.join().unwrap();
    assert!(outcome.stopped);
    assert_eq!(outcome.events_injected, 2);
    assert_eq!(outcome.released.len(), 2);
    assert!(outcome.stop_observed_after_us.unwrap() < 200_000);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(unsafe { GetAsyncKeyState(0xA2) } as u16 & 0x8000, 0);
    assert_eq!(unsafe { GetAsyncKeyState(1) } as u16 & 0x8000, 0);
    let mut cursor = POINT::default();
    unsafe {
        GetCursorPos(&mut cursor).unwrap();
    }
    assert!(
        (cursor.x - (x + 70)).abs() <= 1 && (cursor.y - (y + 45)).abs() <= 1,
        "停止收尾不应把指针拉回旧位置"
    );
    println!(
        "NATIVE_STOP observed_us={:?} released={} unicode_rounds=3",
        outcome.stop_observed_after_us,
        outcome.released.len()
    );
    service.shutdown();
}
#[test]
#[ignore = "配合独立 Repeat 测试实例，在界面启动 3 轮执行后核对接收消息"]
fn native_app_execution_receiver() {
    let path = std::env::var_os("REPEAT_VERIFY_APP_DB").expect("需要隔离的 Repeat 测试任务文件");
    let store = Store::open(std::path::Path::new(&path)).unwrap();
    let active = store.setting("activeTask").unwrap().unwrap();
    let doc = store.load(&active).unwrap();
    let expected = doc.compile();
    let rounds = 3usize;
    let receiver = Receiver::new(true);
    receiver.clear();
    println!(
        "APP_RECEIVER_READY duration_us={} expected_events={}",
        doc.duration_us,
        expected.len()
    );
    let started = Instant::now();
    let timeout = Duration::from_secs(50);
    let mut first = None;
    let wanted_keys = expected
        .iter()
        .filter(|e| matches!(e.kind, EventKind::KeyDown { .. }))
        .count()
        * rounds;
    let wanted_clicks = expected
        .iter()
        .filter(|e| matches!(e.kind, EventKind::MouseDown { .. }))
        .count()
        * rounds;
    loop {
        let messages = receiver.messages();
        if !messages.is_empty() && first.is_none() {
            first = Some(Instant::now());
        }
        let keys = messages
            .iter()
            .filter(|m| m.msg == WM_KEYDOWN || m.msg == WM_SYSKEYDOWN)
            .count();
        let clicks = messages
            .iter()
            .filter(|m| matches!(m.msg, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN))
            .count();
        if keys >= wanted_keys && clicks >= wanted_clicks {
            let last = expected.last().unwrap().time_us;
            thread::sleep(
                Duration::from_micros((doc.duration_us - last).max(0) as u64)
                    + Duration::from_millis(500),
            );
            break;
        }
        assert!(
            started.elapsed() < timeout,
            "Repeat 没有完成界面的 3 轮执行，未收到完整事件"
        );
        thread::sleep(Duration::from_millis(50));
    }
    let messages = receiver.messages();
    let msg_counts = counts(&messages);
    let keydowns = messages
        .iter()
        .filter(|m| m.msg == WM_KEYDOWN || m.msg == WM_SYSKEYDOWN)
        .count();
    assert_eq!(keydowns, wanted_keys);
    let clicks = messages
        .iter()
        .filter(|m| matches!(m.msg, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN))
        .count();
    assert_eq!(clicks, wanted_clicks);
    let wheels = messages
        .iter()
        .filter(|m| matches!(m.msg, WM_MOUSEWHEEL | WM_MOUSEHWHEEL))
        .count();
    assert_eq!(
        wheels,
        expected
            .iter()
            .filter(|e| matches!(e.kind, EventKind::Wheel { .. }))
            .count()
            * rounds
    );
    let first_key_times: Vec<_> = messages
        .iter()
        .filter(|m| m.msg == WM_KEYDOWN && m.value == 0x41)
        .map(|m| m.at)
        .collect();
    let firsts: Vec<_> = first_key_times.chunks(3).map(|chunk| chunk[0]).collect();
    assert_eq!(firsts.len(), rounds);
    let gaps: Vec<_> = firsts
        .windows(2)
        .map(|w| w[1].duration_since(w[0]).as_micros() as i64)
        .collect();
    for &gap in &gaps {
        assert!(
            (gap - doc.duration_us).abs() < 30_000,
            "循环之间增加了额外等待或吞掉停顿"
        );
    }
    println!(
        "APP_EXECUTION rounds=3 receiver_events={} round_gaps_us={:?} duration_us={}",
        messages.len(),
        gaps,
        doc.duration_us
    );
    let data = serde_json::json!({"rounds":3,"receiverEvents":messages.len(),"messageCounts":msg_counts,"roundGapsUs":gaps,"durationUs":doc.duration_us});
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/app-execution-evidence.json"),
        serde_json::to_vec_pretty(&data).unwrap(),
    )
    .unwrap();
}
unsafe extern "system" fn find_monitor_window(hwnd: HWND, lp: LPARAM) -> windows::core::BOOL {
    unsafe {
        let result = &mut *(lp.0 as *mut usize);
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let mut name = [0u16; 64];
        let len = GetClassNameW(hwnd, &mut name);
        if pid == windows::Win32::System::Threading::GetCurrentProcessId()
            && String::from_utf16_lossy(&name[..len as usize]) == "RepeatInputMonitor"
        {
            *result = hwnd.0 as usize;
            return windows::core::BOOL(0);
        }
        windows::core::BOOL(1)
    }
}
#[test]
#[ignore = "向本进程的原生监视窗口投递环境消息；不实际锁屏或挂起系统"]
fn native_environment_messages_cancel_before_input() {
    for (message, value) in [
        (WM_DISPLAYCHANGE, 0),
        (WM_POWERBROADCAST, PBT_APMSUSPEND as usize),
        (WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK as usize),
    ] {
        let stop = StopSignal::new();
        let copy = stop.clone();
        let service = platform::start_input_service(Arc::new(move |action| {
            if action == platform::HotkeyAction::EnvironmentChanged {
                copy.fail("环境变化测试");
            }
        }))
        .unwrap();
        let mut hwnd = 0usize;
        unsafe {
            let _ = EnumWindows(
                Some(find_monitor_window),
                LPARAM(&mut hwnd as *mut usize as isize),
            );
        }
        assert_ne!(hwnd, 0);
        let mut driver = platform::make_driver(&stop, &platform::snapshot_geometry()).unwrap();
        let sender = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            unsafe {
                PostMessageW(
                    Some(HWND(hwnd as *mut _)),
                    message,
                    WPARAM(value),
                    LPARAM(0),
                )
                .unwrap();
            }
        });
        let outcome = run_replay(
            &ScheduleSpec {
                events: Arc::new(vec![RecordedEvent {
                    seq: 1,
                    time_us: 5_000_000,
                    native_time_ms: 0,
                    kind: EventKind::MouseMove { x: 500, y: 500 },
                }]),
                duration_us: 6_000_000,
                rounds: 1,
            },
            &mut driver,
            &stop,
            LATE_THRESHOLD,
            |_| {},
        );
        sender.join().unwrap();
        assert!(outcome.stopped);
        assert_eq!(outcome.events_injected, 0);
        assert!(outcome.failure.unwrap().reason.contains("环境变化"));
        assert!(outcome.stop_observed_after_us.unwrap() < 200_000);
        service.shutdown();
    }
    println!(
        "NATIVE_ENV display/power/session messages cancelled waiting with zero injected events"
    );
}
