//! Windows 实机契约测试。会向专用接收窗口发送合成输入；显式 --ignored 才运行。
//! 测试数据均为固定样本，不读取、输出用户输入。长样本可设 REPEAT_NATIVE_SECONDS=180。
#![cfg(windows)]
use repeat_lib::{
    capture::{Hotkeys, RecordingBuffer},
    document::{ActionKind, EditRequest, RawRecording, TaskDocument},
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
    process::Command,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM},
        Graphics::Gdi::{
            CDS_TYPE, ChangeDisplaySettingsExW, DEVMODEW, DISP_CHANGE_SUCCESSFUL,
            ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE, EnumDisplaySettingsW,
        },
        System::{
            LibraryLoader::GetModuleHandleW,
            Shutdown::LockWorkStation,
            Threading::{
                CREATE_WAITABLE_TIMER_MANUAL_RESET, CreateWaitableTimerExW, SetWaitableTimer,
                TIMER_ALL_ACCESS,
            },
        },
        UI::{Input::KeyboardAndMouse::*, Shell::ShellExecuteW, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};
#[derive(Clone, Debug)]
struct Received {
    msg: u32,
    value: usize,
    x: i32,
    y: i32,
    /// 原始 lParam：键盘消息的位 16-23 是扫描码、位 24 是扩展键标志。
    lp: isize,
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
            lp: lp.0,
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
    // 零间隔的按下-抬起对在系统繁忙时可能不触发 RegisterHotKey，与真实按键一致地保持 60ms。
    thread::sleep(Duration::from_millis(60));
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
            let mut title = [0u16; 128];
            let len = unsafe { GetWindowTextW(GetForegroundWindow(), &mut title) };
            let thief = String::from_utf16_lossy(&title[..len as usize]);
            return Err(format!("测试接收窗口失焦（前台：{thief}），未发送输入"));
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

/// 用真实系统事件验证环境变化取消：接线与应用一致（监视窗口回调直接 fail 停止信号），
/// 唯一计划事件在 5 秒后，触发动作在 500 毫秒后执行。返回执行结果。
fn environment_replay_with_trigger(
    trigger: impl FnOnce() + Send + 'static,
) -> repeat_lib::scheduler::ExecutionOutcome {
    let stop = StopSignal::new();
    let copy = stop.clone();
    let service = platform::start_input_service(Arc::new(move |action| {
        if action == platform::HotkeyAction::EnvironmentChanged {
            copy.fail("桌面会话、显示布局或电源状态发生变化，已停止执行");
        }
    }))
    .unwrap();
    let mut driver = platform::make_driver(&stop, &platform::snapshot_geometry()).unwrap();
    let sender = thread::spawn(move || {
        thread::sleep(Duration::from_millis(500));
        trigger();
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
    service.shutdown();
    outcome
}

#[test]
#[ignore = "向专用接收窗口发送固定输入；核对键盘编辑（特殊键名/原样确认/重复按下）后的真实执行"]
fn native_keys_edit_preservation() {
    let receiver = Receiver::new(true);
    let geometry = platform::snapshot_geometry();
    let scanned = |seq: u64, time_us: i64, vk: u32, scan: u32, down: bool| RecordedEvent {
        seq,
        time_us,
        native_time_ms: 0,
        kind: if down {
            EventKind::KeyDown {
                vk,
                scan,
                extended: false,
            }
        } else {
            EventKind::KeyUp {
                vk,
                scan,
                extended: false,
            }
        },
    };
    let mut doc = TaskDocument::new(
        "键盘编辑终审".into(),
        geometry.clone(),
        platform::current_layout_klid(),
    );
    doc.replace_recording(RawRecording {
        gestures: Default::default(),
        started_at_wall_ms: 0,
        duration_us: 500_000,
        events: vec![
            scanned(1, 0, 0xA2, 0x1D, true),
            scanned(2, 100_000, 0x6B, 0x4E, true),
            scanned(3, 200_000, 0x6B, 0x4E, true),
            scanned(4, 300_000, 0x6B, 0x4E, false),
            scanned(5, 400_000, 0xA2, 0x1D, false),
        ],
        geometry: geometry.clone(),
        layout_klid: platform::current_layout_klid(),
    });
    let ActionKind::Keys { chord } = &doc.actions[0].kind else {
        panic!("样本应归并为键盘动作");
    };
    assert_eq!(chord, "左Ctrl + 小键盘+");
    assert!(
        doc.actions[0].view().detail.contains("1 次重复按下"),
        "重复按下应如实标注：{}",
        doc.actions[0].view().detail
    );
    let before = serde_json::to_string(&doc).unwrap();
    let id = doc.actions[0].id.clone();
    doc.apply(EditRequest::UpdateKeys {
        action_id: id.clone(),
        chord: "左Ctrl + 小键盘+".into(),
    })
    .unwrap();
    assert_eq!(
        before,
        serde_json::to_string(&doc).unwrap(),
        "原样确认不得重建事件（保留重复按下、扫描码与真实时间）"
    );

    let received_vk = |receiver: &Receiver| -> Vec<(u32, usize, u16, bool)> {
        receiver
            .messages()
            .iter()
            .filter(|m| m.msg != WM_CHAR)
            .map(|m| {
                let scan = ((m.lp >> 16) & 0xFF) as u16;
                let extended = (m.lp >> 24) & 1 == 1;
                (m.msg, m.value, scan, extended)
            })
            .collect()
    };
    receiver.clear();
    let stop = StopSignal::new();
    let mut driver = FixtureDriver {
        inner: platform::make_driver(&stop, &geometry).unwrap(),
        target: receiver.hwnd,
    };
    let outcome = run_replay(
        &ScheduleSpec {
            events: Arc::new(doc.compile()),
            duration_us: doc.duration_us,
            rounds: 1,
        },
        &mut driver,
        &stop,
        LATE_THRESHOLD,
        |_| {},
    );
    assert!(outcome.failure.is_none(), "{:?}", outcome.failure);
    assert_eq!(outcome.events_injected, 5);
    // WM_KEYDOWN 的 wParam 对修饰键是通用 VK（真实硬件左 Ctrl 同样是 VK_CONTROL）；
    // 物理键身份由 lParam 扫描码与扩展位表达：左 Ctrl = 扫描码 0x1D 非扩展。
    assert_eq!(
        received_vk(&receiver),
        vec![
            (WM_KEYDOWN, 0x11, 0x1D, false),
            (WM_KEYDOWN, 0x6B, 0x4E, false),
            (WM_KEYDOWN, 0x6B, 0x4E, false),
            (WM_KEYUP, 0x6B, 0x4E, false),
            (WM_KEYUP, 0x11, 0x1D, false),
        ],
        "原样确认后的真实执行应保留重复按下与原始扫描码"
    );

    doc.apply(EditRequest::UpdateKeys {
        action_id: id,
        chord: "右Ctrl + `".into(),
    })
    .unwrap();
    let ActionKind::Keys { chord } = &doc.actions[0].kind else {
        panic!("编辑后仍是键盘动作");
    };
    assert_eq!(chord, "右Ctrl + `");
    doc.validate_execution().unwrap();
    receiver.clear();
    let stop = StopSignal::new();
    let mut driver = FixtureDriver {
        inner: platform::make_driver(&stop, &geometry).unwrap(),
        target: receiver.hwnd,
    };
    let outcome = run_replay(
        &ScheduleSpec {
            events: Arc::new(doc.compile()),
            duration_us: doc.duration_us,
            rounds: 1,
        },
        &mut driver,
        &stop,
        LATE_THRESHOLD,
        |_| {},
    );
    assert!(outcome.failure.is_none(), "{:?}", outcome.failure);
    assert_eq!(outcome.events_injected, 4);
    // 右 Ctrl 的物理身份 = 扫描码 0x1D + 扩展位；反引号 = 扫描码 0x29。
    assert_eq!(
        received_vk(&receiver),
        vec![
            (WM_KEYDOWN, 0x11, 0x1D, true),
            (WM_KEYDOWN, 0xC0, 0x29, false),
            (WM_KEYUP, 0xC0, 0x29, false),
            (WM_KEYUP, 0x11, 0x1D, true),
        ],
        "编辑后的和弦应按解析结果注入（右Ctrl + `，含扩展位）"
    );
    println!(
        "NATIVE_KEYS_EDIT as_is_events=5 edited_events=4 receiver_sequences=2 p95_us={}",
        compute_stats(&outcome.timing).p95_abs_us
    );
    let evidence = serde_json::json!({
        "asIs": {"events": 5, "received": [
            "keydown vk=0x11 scan=0x1D", "keydown vk=0x6B scan=0x4E",
            "keydown vk=0x6B scan=0x4E (重复)", "keyup vk=0x6B scan=0x4E", "keyup vk=0x11 scan=0x1D"]},
        "edited": {"chord": "右Ctrl + `", "events": 4, "received": [
            "keydown vk=0x11 scan=0x1D ext=1", "keydown vk=0xC0 scan=0x29",
            "keyup vk=0xC0 scan=0x29", "keyup vk=0x11 scan=0x1D ext=1"]},
        "editedStats": compute_stats(&outcome.timing),
        "note": "修饰键的 WM_KEYDOWN wParam 为通用 VK（与硬件一致）；物理身份用 lParam 扫描码+扩展位核对",
    });
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/keys-edit-evidence.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}

/// 结束时恢复原始分辨率（无论断言结果如何）。
struct RestoreDisplayMode(DEVMODEW);
impl Drop for RestoreDisplayMode {
    fn drop(&mut self) {
        unsafe {
            let dm = self.0;
            let _ = ChangeDisplaySettingsExW(PCWSTR::null(), Some(&dm), None, CDS_TYPE(0), None);
        }
    }
}

#[test]
#[ignore = "真实切换本机屏幕分辨率约 1-2 秒后恢复；验证真实显示变化取消执行（屏幕会闪两次）"]
fn native_environment_display_change_cancels() {
    unsafe {
        let mut current = DEVMODEW {
            dmSize: std::mem::size_of::<DEVMODEW>() as u16,
            ..Default::default()
        };
        assert!(
            EnumDisplaySettingsW(PCWSTR::null(), ENUM_CURRENT_SETTINGS, &mut current).as_bool()
        );
        let mut candidate = None;
        for i in 0..128u32 {
            let mut dm = DEVMODEW {
                dmSize: std::mem::size_of::<DEVMODEW>() as u16,
                ..Default::default()
            };
            if !EnumDisplaySettingsW(PCWSTR::null(), ENUM_DISPLAY_SETTINGS_MODE(i), &mut dm)
                .as_bool()
            {
                break;
            }
            if (dm.dmPelsWidth, dm.dmPelsHeight) != (current.dmPelsWidth, current.dmPelsHeight) {
                candidate = Some(dm);
                break;
            }
        }
        let candidate = candidate.expect("本机没有可用的其他分辨率模式，无法验证真实显示变化");
        let before = platform::snapshot_geometry();
        let restore = RestoreDisplayMode(current);
        let candidate_dims = (candidate.dmPelsWidth, candidate.dmPelsHeight);
        let outcome = environment_replay_with_trigger(move || {
            let rc =
                ChangeDisplaySettingsExW(PCWSTR::null(), Some(&candidate), None, CDS_TYPE(0), None);
            assert_eq!(rc, DISP_CHANGE_SUCCESSFUL, "临时切换分辨率失败：{rc:?}");
        });
        assert!(outcome.stopped);
        assert_eq!(outcome.events_injected, 0);
        assert!(
            outcome
                .failure
                .as_ref()
                .unwrap()
                .reason
                .contains("显示布局"),
            "失败原因应指向显示布局：{:?}",
            outcome.failure
        );
        let during = platform::snapshot_geometry();
        assert!(
            !before.same_layout(&during),
            "切换后桌面几何应已变化：{before:?} -> {during:?}"
        );
        let current_dims = (
            during.monitors.first().unwrap().width,
            during.monitors.first().unwrap().height,
        );
        drop(restore);
        thread::sleep(Duration::from_millis(400));
        let after = platform::snapshot_geometry();
        assert!(
            before.same_layout(&after),
            "恢复后桌面几何应回到原样：{before:?} -> {after:?}"
        );
        println!(
            "NATIVE_ENV_DISPLAY candidate={candidate_dims:?} measured={current_dims:?} injected={} cancelled_with_reason=true",
            outcome.events_injected
        );
    }
}

#[test]
#[ignore = "真实锁定桌面（等同 Win+L）；测试结束后桌面保持锁定，需要你解锁返回"]
fn native_environment_lock_cancels() {
    let outcome = environment_replay_with_trigger(|| {
        unsafe { LockWorkStation() }.expect("无法锁定工作站");
    });
    assert!(outcome.stopped);
    assert_eq!(outcome.events_injected, 0);
    assert!(
        outcome
            .failure
            .as_ref()
            .unwrap()
            .reason
            .contains("桌面会话"),
        "失败原因应指向桌面会话：{:?}",
        outcome.failure
    );
    println!(
        "NATIVE_ENV_LOCK observed_after_us={} (桌面已锁定，请解锁返回)",
        outcome.stop_observed_after_us.unwrap()
    );
}

#[test]
#[ignore = "真实让本机睡眠（25 秒后由唤醒计时器自动恢复，或按任意键唤醒）；验证挂起取消执行"]
fn native_environment_suspend_resumes_and_cancels() {
    unsafe {
        // 唤醒计时器（可选）：电源策略禁用唤醒计时器时返回 None，改为提示手动唤醒。
        let wake_timer = CreateWaitableTimerExW(
            None,
            None,
            CREATE_WAITABLE_TIMER_MANUAL_RESET,
            TIMER_ALL_ACCESS.0,
        )
        .ok()
        .and_then(|timer| {
            // 25 秒（100 纳秒单位，负值表示相对时间）；bResume=true 使系统到点自动唤醒。
            let due: i64 = -250_000_000i64;
            if SetWaitableTimer(timer, &due, 0, None, None, true).is_ok() {
                Some(timer)
            } else {
                let _ = CloseHandle(timer);
                None
            }
        });
        if wake_timer.is_some() {
            println!("NATIVE_ENV_SUSPEND 0.5 秒后本机将睡眠约 25 秒，随后自动唤醒……");
        } else {
            println!("NATIVE_ENV_SUSPEND 唤醒计时器不可用：0.5 秒后本机将睡眠，请按任意键唤醒……");
        }
        let outcome = environment_replay_with_trigger(|| {
            let status = Command::new("rundll32.exe")
                .args(["powrprof.dll,SetSuspendState", "0", "1", "0"])
                .status()
                .expect("无法发起睡眠");
            assert!(status.success(), "发起睡眠失败：{status}");
        });
        assert!(outcome.stopped);
        assert_eq!(outcome.events_injected, 0);
        assert!(
            outcome
                .failure
                .as_ref()
                .unwrap()
                .reason
                .contains("电源状态"),
            "失败原因应指向电源状态：{:?}",
            outcome.failure
        );
        println!(
            "NATIVE_ENV_SUSPEND observed_after_us={} (已从睡眠恢复)",
            outcome.stop_observed_after_us.unwrap()
        );
        if let Some(timer) = wake_timer {
            let _ = CloseHandle(timer);
        }
    }
}

/// UIPI 接收器 helper：以 REPEAT_UIPI_HELPER_DIR 环境变量运行本测试二进制时进入。
/// 创建接收窗口、尽力获得前台焦点，写入 ready.json；监听若干秒后写入 result.json 退出。
fn uipi_helper_main(dir: std::path::PathBuf) {
    platform::init_process_dpi();
    let elevated = Command::new("net.exe")
        .arg("session")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let receiver = Receiver::new(false);
    let focus = || unsafe {
        let _ = SendMessageW(HWND(receiver.hwnd as *mut _), WM_APP + 12, None, None);
    };
    focus();
    thread::sleep(Duration::from_millis(300));
    let focused = unsafe { GetForegroundWindow().0 as usize } == receiver.hwnd;
    std::fs::write(
        dir.join("ready.json"),
        serde_json::json!({"hwnd": receiver.hwnd, "focused": focused, "elevated": elevated})
            .to_string(),
    )
    .unwrap();
    println!(
        "UIPI_HELPER elevated={elevated} focused={focused} hwnd={}",
        receiver.hwnd
    );
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(8) {
        focus();
        thread::sleep(Duration::from_millis(200));
    }
    let messages: Vec<(u32, usize)> = receiver
        .messages()
        .iter()
        .map(|m| (m.msg, m.value))
        .collect();
    std::fs::write(
        dir.join("result.json"),
        serde_json::json!({
            "elevated": elevated,
            "focused": focused,
            "messages": messages,
        })
        .to_string(),
    )
    .unwrap();
}

fn uipi_wait_file(path: &std::path::Path, timeout: Duration) -> serde_json::Value {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(text) = std::fs::read_to_string(path)
            && let Ok(value) = serde_json::from_str(&text)
        {
            return value;
        }
        thread::sleep(Duration::from_millis(150));
    }
    panic!("等待 {:?} 超时：{}", timeout, path.display());
}

fn uipi_assert_foreground(hwnd: usize, timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if unsafe { GetForegroundWindow().0 as usize } == hwnd {
            return;
        }
        thread::sleep(Duration::from_millis(120));
    }
    panic!("接收窗口 {hwnd} 未能在 {timeout:?} 内保持前台焦点，未发送测试输入");
}

fn uipi_inject_sample_keys() {
    for vk in [0x41u32, 0x6B, 0x09] {
        external(key(vk, true));
        external(key(vk, false));
        thread::sleep(Duration::from_millis(60));
    }
}

#[test]
#[ignore = "需要一次 UAC 确认启动管理员接收器；验证普通权限注入到管理员目标被 UIPI 静默拦截"]
fn native_uipi_restricted_target_receives_nothing() {
    if let Some(dir) = std::env::var_os("REPEAT_UIPI_HELPER_DIR") {
        uipi_helper_main(std::path::PathBuf::from(dir));
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let args = "--exact native_uipi_restricted_target_receives_nothing --ignored --nocapture --test-threads=1";
    let dir = std::env::temp_dir().join(format!("repeat-uipi-{}", repeat_lib::document::new_id()));
    let control_dir = dir.join("control");
    let elevated_dir = dir.join("elevated");
    std::fs::create_dir_all(&control_dir).unwrap();
    std::fs::create_dir_all(&elevated_dir).unwrap();
    let key_messages = |value: &serde_json::Value| -> usize {
        value["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| {
                let msg = m[0].as_u64().unwrap() as u32;
                let vk = m[1].as_u64().unwrap_or(0);
                matches!(msg, WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP)
                    && [0x41u64, 0x6B, 0x09].contains(&vk)
            })
            .count()
    };

    // 对照组：同一接收器以普通权限运行，注入必须到达（证明注入与接收链路本身可用）。
    let mut control_child = Command::new(&exe)
        .args(args.split_whitespace())
        .env("REPEAT_UIPI_HELPER_DIR", &control_dir)
        .spawn()
        .unwrap();
    let ready = uipi_wait_file(&control_dir.join("ready.json"), Duration::from_secs(20));
    assert!(
        !ready["elevated"].as_bool().unwrap(),
        "对照组不应是管理员权限"
    );
    uipi_assert_foreground(
        ready["hwnd"].as_u64().unwrap() as usize,
        Duration::from_secs(5),
    );
    uipi_inject_sample_keys();
    let control = uipi_wait_file(&control_dir.join("result.json"), Duration::from_secs(20));
    let _ = control_child.wait();
    let control_keys = key_messages(&control);
    assert!(
        control_keys >= 6,
        "对照组应收到全部固定按键消息，实际 {control_keys} 条"
    );

    // 实验组：UAC 提权同一接收器（会弹 UAC 确认），普通权限注入同一样本。
    // ShellExecuteW 继承本进程环境，因此先设置 helper 目录再启动。
    unsafe { std::env::set_var("REPEAT_UIPI_HELPER_DIR", &elevated_dir) };
    let exe_w: Vec<u16> = exe
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let args_w: Vec<u16> = args.encode_utf16().chain(std::iter::once(0)).collect();
    let run = unsafe {
        ShellExecuteW(
            None,
            w!("runas"),
            PCWSTR(exe_w.as_ptr()),
            PCWSTR(args_w.as_ptr()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    assert!(
        run.0 as isize > 32,
        "管理员接收器未能启动（UAC 被拒绝或失败）"
    );
    let ready = uipi_wait_file(&elevated_dir.join("ready.json"), Duration::from_secs(90));
    assert!(
        ready["elevated"].as_bool().unwrap(),
        "UAC 后接收器必须是管理员权限"
    );
    assert!(
        ready["focused"].as_bool().unwrap(),
        "管理员接收器应已获得前台焦点"
    );
    uipi_assert_foreground(
        ready["hwnd"].as_u64().unwrap() as usize,
        Duration::from_secs(5),
    );
    uipi_inject_sample_keys();
    let elevated = uipi_wait_file(&elevated_dir.join("result.json"), Duration::from_secs(30));
    let elevated_keys = key_messages(&elevated);
    assert_eq!(
        elevated_keys, 0,
        "管理员目标不应收到任何注入按键；SendInput 全部返回成功但消息被 UIPI 静默拦截"
    );
    println!(
        "NATIVE_UIPI control_messages={control_keys} elevated_messages={elevated_keys} sendinput_returned_success=true"
    );
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/uipi-evidence.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "control": {"elevated": false, "keyMessages": control_keys},
            "elevated": {"elevated": true, "keyMessages": elevated_keys},
            "sample": "A / 小键盘+ / Tab 各一次按下抬起",
            "conclusion": "普通权限 SendInput 返回成功，管理员目标 0 条消息：UIPI 静默拦截",
        }))
        .unwrap(),
    )
    .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
