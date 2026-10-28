//! 独立桌面验收：由操作者开始录制、完成一次普通点击，再点击状态条停止。
#![cfg(windows)]

use repeat_lib::{
    document::TaskDocument,
    event::{EventKind, MouseButton},
    platform,
    storage::Store,
};
use std::{
    process::Command,
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HWND, POINT},
        Graphics::Gdi::ClientToScreen,
        UI::WindowsAndMessaging::{FindWindowExW, GetWindowThreadProcessId, IsWindowVisible},
    },
    core::w,
};

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
    Store::open(&db).unwrap().insert(&doc).unwrap();
    let child = Command::new(executable)
        .env("REPEAT_DATA_DIR", &directory)
        .spawn()
        .unwrap();
    let child = ChildGuard(child);
    println!(
        "RECORDING_OVERLAY_READY pid={} 请在新 Repeat 中开始录制，然后在主窗口空白处点击一次，最后点击底部停止。",
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
    println!(
        "RECORDING_OVERLAY_VISIBLE bounds={bounds:?}: 请在主窗口空白处点击一次，再点击底部停止。"
    );
    let wait = Instant::now();
    while overlay_for_process(child.0.id()).is_some() {
        assert!(wait.elapsed() < Duration::from_secs(120), "未等到录制停止");
        thread::sleep(Duration::from_millis(50));
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
    let raw = saved.raw.unwrap();
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
    let evidence = serde_json::json!({"fixture":"isolated Tauri recording; physical prior click and physical overlay stop", "stopBounds":bounds, "leftButtonEvents":clicks.iter().map(|(seq,time,kind,x,y)| serde_json::json!({"seq":seq,"timeUs":time,"kind":kind,"x":x,"y":y})).collect::<Vec<_>>()});
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/recording-overlay-evidence.json"),
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
    println!("RECORDING_OVERLAY_EVIDENCE {evidence}");
}
