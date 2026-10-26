//! 真机原生链路集成测试（M1-03/M1-04 事件级证据）。
//!
//! 与单元测试的模拟驱动不同，这里走真实的 Windows 低级钩子与 SendInput：
//! - `native_hooks_capture_injected_input`：安装钩子后用 SendInput 注入外部输入（不带
//!   本工具标记），验证钩子线程真实捕获到事件且序号/时间有序。
//! - `native_replay_sendinput_absolute_deadlines`：用真实驱动（高精度可等待计时器 +
//!   SendInput）回放一个短时间轴，验证注入数量与计划偏差。
//!
//! 这些测试会移动真实桌面上的鼠标指针（不点击、不按键），只应在本机交互会话运行：
//! `cargo test --test native_roundtrip -- --ignored --nocapture`。CI 不运行。

#![cfg(windows)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT, SendInput,
};

use repeat_lib::capture::RecordingBuffer;
use repeat_lib::event::{EventKind, RecordedEvent};
use repeat_lib::geometry::DesktopGeometry;
use repeat_lib::platform;
use repeat_lib::scheduler::{LATE_THRESHOLD, ScheduleSpec, StopSignal, run_replay};

fn inject_absolute_move(geo: &DesktopGeometry) {
    let v = geo.virtual_screen().expect("虚拟桌面几何");
    // 目标取在虚拟桌面内部（避开边缘），依次移动以便核对顺序。
    let targets = [
        (v.x + v.width / 4, v.y + v.height / 4),
        (v.x + v.width / 2, v.y + v.height / 2),
        (v.x + 3 * v.width / 4, v.y + v.height / 3),
    ];
    for (tx, ty) in targets {
        let (nx, ny) = repeat_lib::geometry::to_normalized_absolute(tx, ty, &v);
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: nx,
                    dy: ny,
                    mouseData: 0,
                    // 不带 REPEAT_INJECT_MARKER：对钩子而言这就是“外部输入”。
                    dwFlags: MOUSE_EVENT_FLAGS(
                        MOUSEEVENTF_MOVE.0 | MOUSEEVENTF_ABSOLUTE.0 | MOUSEEVENTF_VIRTUALDESK.0,
                    ),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let sent = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
        assert_eq!(sent, 1, "SendInput 注入失败");
        std::thread::sleep(Duration::from_millis(80));
    }
}

#[test]
#[ignore = "向真实桌面注入鼠标移动以验证钩子捕获；仅在本机交互会话运行"]
fn native_hooks_capture_injected_input() {
    let service = platform::start_input_service(Arc::new(|_| {})).expect("输入服务启动失败");
    let buffer = Arc::new(Mutex::new(RecordingBuffer::default()));
    buffer.lock().unwrap().begin();
    service.start_capture(buffer.clone()).unwrap();
    std::thread::sleep(Duration::from_millis(500)); // 等钩子安装完成
    println!("服务状态: {:?}", service.status_lines());

    inject_absolute_move(&platform::snapshot_geometry());

    std::thread::sleep(Duration::from_millis(300));
    let (cb, with_buf) = platform::windows::input_service::hook_diagnostics();
    println!("钩子回调次数 {cb}，其中拿到缓冲 {with_buf}");
    service.stop_capture().unwrap();
    std::thread::sleep(Duration::from_millis(300)); // 等卸载消息处理完
    let finished = buffer.lock().unwrap().finish();
    service.shutdown();

    let moves: Vec<&RecordedEvent> = finished
        .events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::MouseMove { .. }))
        .collect();
    println!(
        "捕获事件总数 {}，其中移动 {}；前 5 条：{:?}",
        finished.events.len(),
        moves.len(),
        finished
            .events
            .iter()
            .take(5)
            .map(|e| (e.seq, e.time_us, e.kind))
            .collect::<Vec<_>>()
    );
    assert!(
        moves.len() >= 3,
        "钩子应捕获到至少 3 次注入的移动，实际 {}（事件总数 {}）",
        moves.len(),
        finished.events.len()
    );
    let times: Vec<i64> = finished.events.iter().map(|e| e.time_us).collect();
    assert!(
        times.windows(2).all(|w| w[0] <= w[1]),
        "time_us 必须非递减：{times:?}"
    );
    let seqs: Vec<u64> = finished.events.iter().map(|e| e.seq).collect();
    assert_eq!(
        seqs,
        (1..=seqs.len() as u64).collect::<Vec<u64>>(),
        "序号必须从 1 连续递增"
    );
}

#[test]
#[ignore = "真实 SendInput 注入与计时器验证；仅在本机交互会话运行"]
fn native_replay_sendinput_absolute_deadlines() {
    let geo = platform::snapshot_geometry();
    let v = geo.virtual_screen().expect("虚拟桌面几何");
    let mk = |seq: u64, t_us: i64, x: i32, y: i32| RecordedEvent {
        seq,
        time_us: t_us,
        native_time_ms: 0,
        kind: EventKind::MouseMove { x, y },
    };
    let events = vec![
        mk(1, 0, v.x + v.width / 5, v.y + v.height / 5),
        mk(2, 150_000, v.x + 2 * v.width / 5, v.y + v.height / 2),
        mk(3, 300_000, v.x + 3 * v.width / 5, v.y + 4 * v.height / 5),
    ];
    let spec = ScheduleSpec {
        events: Arc::new(events),
        duration_us: 400_000,
        rounds: 1,
    };
    let stop = StopSignal::new();
    let mut driver = platform::make_driver(&stop, &geo).expect("驱动构造失败");

    let outcome = run_replay(&spec, &mut driver, &stop, LATE_THRESHOLD, |_| {});
    println!(
        "注入 {} 条，完成 {} 轮；计时：{:?}",
        outcome.events_injected, outcome.rounds_completed, outcome.timing
    );
    assert!(
        outcome.failure.is_none(),
        "不应有失败：{:?}",
        outcome.failure
    );
    assert_eq!(outcome.events_injected, 3);
    assert_eq!(outcome.rounds_completed, 1);
    for t in &outcome.timing {
        let dev_us = (t.actual_us - t.planned_us).abs();
        assert!(
            dev_us < 200_000,
            "注入偏差 {dev_us}us 超过 200ms（原生路径异常）"
        );
    }
}

/// 最小对照：主线程直接装 WH_MOUSE_LL、PeekMessage 泵、注入一次移动。
/// 用于区分“服务线程实现问题”与“进程环境（完整性级别/会话）问题”。
#[test]
#[ignore = "最小钩子对照，同样注入真实鼠标移动"]
fn minimal_ll_hook_on_current_thread() {
    use std::sync::atomic::{AtomicU64, Ordering};
    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, HOOKPROC, MSG, PM_REMOVE, PeekMessageW, SetWindowsHookExW,
        UnhookWindowsHookEx, WH_MOUSE_LL,
    };

    static COUNT: AtomicU64 = AtomicU64::new(0);
    unsafe extern "system" fn proc(_c: i32, _w: WPARAM, _l: LPARAM) -> LRESULT {
        unsafe {
            COUNT.fetch_add(1, Ordering::Relaxed);
            CallNextHookEx(None, _c, _w, _l)
        }
    }

    unsafe {
        let module = GetModuleHandleW(None).expect("GetModuleHandleW");
        let hook: HOOKPROC = Some(proc);
        let h = SetWindowsHookExW(WH_MOUSE_LL, hook, Some(HINSTANCE(module.0)), 0)
            .expect("SetWindowsHookExW 应成功");
        assert!(!h.is_invalid(), "钩子句柄无效");

        let mut msg = MSG::default();
        let deadline = std::time::Instant::now() + Duration::from_millis(1500);
        let geo = platform::snapshot_geometry();
        let mut injected = 0u32;
        while std::time::Instant::now() < deadline {
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE);
            if injected < 3 {
                inject_absolute_move(&geo);
                injected += 1;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        let _ = UnhookWindowsHookEx(h);
        let count = COUNT.load(Ordering::Relaxed);
        println!("最小对照：注入 {injected} 次，钩子回调 {count} 次");
        assert!(
            count > 0,
            "主线程最小钩子也收不到回调：进程环境问题（完整性级别/会话/桌面）"
        );
    }
}
