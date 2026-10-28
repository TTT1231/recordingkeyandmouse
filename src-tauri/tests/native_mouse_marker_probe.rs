//! 独立固定样本：对照输入标记与低级鼠标钩子回调。
//! 只保存本测试注入的鼠标事件，不保存其他桌面输入。
#![cfg(windows)]

use repeat_lib::{geometry::to_normalized_absolute, platform};
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock, mpsc},
    thread,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{FILETIME, HINSTANCE, LPARAM, LRESULT, POINT, WPARAM},
    System::{
        LibraryLoader::GetModuleHandleW,
        Threading::{GetCurrentThread, GetCurrentThreadId, GetThreadTimes},
    },
    UI::{
        Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_MOUSE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE,
            MOUSEEVENTF_MOVE_NOCOALESCE, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT, SendInput,
        },
        WindowsAndMessaging::{
            CallNextHookEx, DispatchMessageW, GetCursorPos, GetMessageW, MSG, MSLLHOOKSTRUCT,
            PostThreadMessageW, SetCursorPos, SetWindowsHookExW, TranslateMessage,
            UnhookWindowsHookEx, WH_MOUSE_LL, WM_MOUSEMOVE, WM_QUIT,
        },
    },
};

const COUNT: u32 = 1000;
const TARGET_INTERVAL_US: u64 = 2000;
const MARKERS: [usize; 2] = [0x4254_0000, 0x4255_0000];
const TIME_BASE: u32 = 0x5a00_0000;
struct HookObservation {
    marker: usize,
    time: u32,
    entered: Instant,
    exited: Instant,
}
struct SlowCall {
    index: u32,
    entered: Instant,
    exited: Instant,
    cpu_us: Option<u64>,
}
static OBSERVED: OnceLock<Mutex<Vec<HookObservation>>> = OnceLock::new();

fn current_thread_cpu_us() -> Option<u64> {
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe {
        GetThreadTimes(
            GetCurrentThread(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
        .ok()?
    };
    let ticks =
        |value: FILETIME| (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime);
    Some((ticks(kernel) + ticks(user)) / 10)
}

unsafe extern "system" fn hook(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    let entered = Instant::now();
    let mut sample = None;
    if code >= 0 && message.0 as u32 == WM_MOUSEMOVE {
        let info = unsafe { &*(data.0 as *const MSLLHOOKSTRUCT) };
        let marker = info.dwExtraInfo;
        if MARKERS
            .iter()
            .any(|base| (*base..*base + COUNT as usize).contains(&marker))
        {
            sample = Some((marker, info.time));
        }
    }
    let result = unsafe { CallNextHookEx(None, code, message, data) };
    if let Some((marker, time)) = sample
        && let Some(observed) = OBSERVED.get()
    {
        observed.lock().unwrap().push(HookObservation {
            marker,
            time,
            entered,
            exited: Instant::now(),
        });
    }
    result
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
#[ignore = "使用 SendInput 移动真实鼠标，需交互桌面和操作者空档"]
fn native_mouse_marker_probe() {
    platform::init_process_dpi();
    let mut original = POINT::default();
    unsafe {
        GetCursorPos(&mut original).unwrap();
    }
    let _restore = CursorRestore(original);
    OBSERVED
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap()
        .clear();
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || unsafe {
        let module = GetModuleHandleW(None).unwrap();
        let hook =
            SetWindowsHookExW(WH_MOUSE_LL, Some(hook), Some(HINSTANCE(module.0)), 0).unwrap();
        tx.send(GetCurrentThreadId()).unwrap();
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = UnhookWindowsHookEx(hook);
    });
    let thread_id = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("钩子启动超时");
    let geo = platform::snapshot_geometry();
    let virt = geo.virtual_screen().expect("桌面几何不可用");
    let mut phases = Vec::new();
    for (phase, base) in MARKERS.iter().copied().enumerate() {
        let mut send_failures = Vec::new();
        let mut slow_calls = Vec::new();
        let mut send_lateness_us = Vec::with_capacity(COUNT as usize);
        let start = Instant::now();
        for i in 0..COUNT {
            let deadline = start + Duration::from_micros(u64::from(i) * TARGET_INTERVAL_US);
            if let Some(wait) = deadline.checked_duration_since(Instant::now()) {
                thread::sleep(wait);
            }
            let x = virt.x + virt.width / 4 + (i % 250) as i32;
            let y = virt.y + virt.height / 4 + ((i / 250) % 2) as i32 * 20;
            let (dx, dy) = to_normalized_absolute(x, y, &virt);
            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx,
                        dy,
                        mouseData: 0,
                        dwFlags: MOUSE_EVENT_FLAGS(
                            MOUSEEVENTF_MOVE.0
                                | MOUSEEVENTF_ABSOLUTE.0
                                | MOUSEEVENTF_VIRTUALDESK.0
                                | if phase == 1 {
                                    MOUSEEVENTF_MOVE_NOCOALESCE.0
                                } else {
                                    0
                                },
                        ),
                        time: TIME_BASE + i + 1,
                        dwExtraInfo: base + i as usize,
                    },
                },
            };
            let cpu_before = current_thread_cpu_us();
            let before = Instant::now();
            send_lateness_us.push(before.saturating_duration_since(deadline).as_micros());
            let sent = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
            let after = Instant::now();
            let elapsed = after.duration_since(before).as_micros();
            let cpu_after = current_thread_cpu_us();
            if sent != 1 {
                send_failures.push(i);
            }
            if elapsed > 100_000 {
                slow_calls.push(SlowCall {
                    index: i,
                    entered: before,
                    exited: after,
                    cpu_us: cpu_before
                        .zip(cpu_after)
                        .map(|(before, after)| after.saturating_sub(before)),
                });
            }
        }
        thread::sleep(Duration::from_millis(300));
        phases.push((send_failures, slow_calls, send_lateness_us));
    }
    unsafe {
        PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)).unwrap();
    }
    worker.join().unwrap();
    let observed = OBSERVED.get().unwrap().lock().unwrap();
    let mut summaries = Vec::new();
    for (phase, base) in MARKERS.iter().copied().enumerate() {
        let mut actual = BTreeMap::<usize, Vec<u32>>::new();
        for event in observed
            .iter()
            .filter(|event| (base..base + COUNT as usize).contains(&event.marker))
        {
            actual.entry(event.marker).or_default().push(event.time);
        }
        let mut missing = Vec::new();
        let mut mismatched_time = Vec::new();
        let mut duplicates = Vec::new();
        for i in 0..COUNT {
            match actual.get(&(base + i as usize)) {
                None => missing.push(i),
                Some(times) => {
                    if times.len() != 1 {
                        duplicates.push(i);
                    }
                    if !times.contains(&(TIME_BASE + i + 1)) {
                        mismatched_time.push(serde_json::json!({"index": i, "expected": TIME_BASE+i+1, "observed": times}));
                    }
                }
            }
        }
        let slow_calls: Vec<_> = phases[phase].1.iter().map(|call| {
            let hook = observed.iter().find(|entry| entry.marker == base + call.index as usize);
            serde_json::json!({"index":call.index, "wallUs":call.exited.duration_since(call.entered).as_micros(), "threadCpuUs":call.cpu_us, "hook":hook.map(|entry| serde_json::json!({"enterAfterCallUs":entry.entered.saturating_duration_since(call.entered).as_micros(), "nextHookDurationUs":entry.exited.saturating_duration_since(entry.entered).as_micros(), "callReturnedBeforeHookExit":call.exited < entry.exited, "afterHookUntilReturnUs":call.exited.saturating_duration_since(entry.exited).as_micros()}))})
        }).collect();
        let lateness = &phases[phase].2;
        let catch_up_samples = lateness
            .iter()
            .filter(|&&us| us >= u128::from(TARGET_INTERVAL_US))
            .count();
        summaries.push(serde_json::json!({"mode": if phase == 0 {"normal"} else {"move_no_coalesce"}, "sent": COUNT, "uniqueMarkersObserved": actual.len(), "missingIndices":missing, "timeMismatches": mismatched_time, "duplicateIndices":duplicates, "sendFailures":phases[phase].0, "slowSendCallsUs":slow_calls, "sendLatenessUs":lateness, "catchUpSamples":catch_up_samples, "maxLatenessUs":lateness.iter().copied().max().unwrap_or(0)}));
    }
    let evidence = serde_json::json!({"fixture":"unique marker mouse moves only; unrelated inputs ignored", "targetIntervalUs":TARGET_INTERVAL_US, "catchUpDefinition":"send starts at least one target interval after its deadline", "phases":summaries});
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/native-mouse-marker-probe-evidence.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    println!("MOUSE_MARKER_PROBE evidence={}", path.display());
    for summary in &summaries {
        assert_eq!(
            summary["sendFailures"].as_array().unwrap().len(),
            0,
            "SendInput 失败: {summary}"
        );
        assert_eq!(
            summary["missingIndices"].as_array().unwrap().len(),
            0,
            "钩子未观察到部分标记: {summary}"
        );
        assert_eq!(
            summary["timeMismatches"].as_array().unwrap().len(),
            0,
            "标记对应时间改变: {summary}"
        );
        assert_eq!(
            summary["duplicateIndices"].as_array().unwrap().len(),
            0,
            "出现重复标记: {summary}"
        );
    }
}
