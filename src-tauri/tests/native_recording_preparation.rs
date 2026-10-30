//! Preparation installs stop listeners without attaching a capture buffer.
//! Uses fixed synthetic mouse moves on an interactive Windows desktop.
#![cfg(windows)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use repeat_lib::capture::RecordingBuffer;
use repeat_lib::platform::{self, RecordingPreparation};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_MOVE, MOUSEINPUT, SendInput,
};

fn move_mouse(dx: i32) {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy: 0,
                dwFlags: MOUSEEVENTF_MOVE,
                ..Default::default()
            },
        },
    };
    assert_eq!(unsafe { SendInput(&[input], size_of::<INPUT>() as i32) }, 1);
    std::thread::sleep(Duration::from_millis(100));
}

#[test]
#[ignore = "向真实 Windows 桌面发送固定鼠标移动，验证准备期没有事件捕获；需交互会话"]
fn native_preparation_has_no_capture_and_cancel_is_scoped() {
    platform::init_process_dpi();
    let service = platform::start_input_service(Arc::new(|_| {})).unwrap();
    let first = RecordingPreparation::new(1);
    service.prepare_recording(Some(first.clone())).unwrap();
    let before = platform::windows::input_service::hook_diagnostics();
    move_mouse(8);
    move_mouse(-8);
    let preparing = platform::windows::input_service::hook_diagnostics();
    assert!(preparing.0 > before.0, "真实钩子必须收到准备期间的输入");
    assert_eq!(preparing.1, before.1, "准备期不得把输入写入录制缓冲");

    let cancelled_action = first.cancel();
    let rejected = Arc::new(Mutex::new(RecordingBuffer::default()));
    rejected.lock().unwrap().begin();
    assert!(service.start_capture(rejected.clone()).is_err());
    assert_eq!(rejected.lock().unwrap().finish().events.len(), 0);

    let second = RecordingPreparation::new(2);
    service.prepare_recording(Some(second.clone())).unwrap();
    // A late first-preparation signal must not affect the next preparation.
    first.cancel();
    assert!(!second.is_cancelled());
    let buffer = Arc::new(Mutex::new(RecordingBuffer::default()));
    buffer.lock().unwrap().begin();
    service.start_capture(buffer.clone()).unwrap();
    move_mouse(9);
    move_mouse(-9);
    service.stop_capture().unwrap();
    let finished = buffer.lock().unwrap().finish();
    service.prepare_recording(None).unwrap();
    service.shutdown();
    assert!(finished.events.len() >= 2, "倒计时结束后的输入必须进入录制");
    assert_eq!(finished.events[0].seq, 1);
    assert!(finished.events[0].time_us < 1_000_000);

    let evidence = serde_json::json!({
        "preparationHookCallbacks": preparing.0 - before.0,
        "preparationBufferedCallbacks": preparing.1 - before.1,
        "cancelledPreparationAction": format!("{cancelled_action:?}"),
        "cancelledCaptureRejected": true,
        "laterPreparationUnaffected": !second.is_cancelled(),
        "recordedEvents": finished.events,
        "durationUs": finished.duration_us,
        "inputSource": "fixed synthetic mouse moves; does not verify physical stop clicks",
    });
    std::fs::create_dir_all("target").unwrap();
    std::fs::write(
        "target/recording-preparation-service-evidence.json",
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}
