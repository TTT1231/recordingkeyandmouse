//! 平台层：隔离 Windows 捕获、注入、坐标和停止操作。
//! 不依赖界面组件或数据库；测试用可替换的时钟/注入驱动（见 `scheduler::ReplayDriver`）。
//! Linux X11 后端尚未实现；非 Windows 平台使用明确报错的占位实现。

use std::sync::{Arc, Mutex};

use crate::capture::RecordingBuffer;

/// Physical screen rectangle of the mouse-transparent replay stop control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StopRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl StopRegion {
    pub fn contains(self, x: i32, y: i32) -> bool {
        let dx = i64::from(x) - i64::from(self.x);
        let dy = i64::from(y) - i64::from(self.y);
        dx >= 0 && dy >= 0 && dx < i64::from(self.width) && dy < i64::from(self.height)
    }
}

static OVERLAY_STOP_REGION: Mutex<Option<StopRegion>> = Mutex::new(None);

pub fn set_overlay_stop_region(region: Option<StopRegion>) {
    *OVERLAY_STOP_REGION.lock().unwrap() = region;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyAction {
    ToggleRecording,
    StopRecording(u64),
    StopExecution,
    EnvironmentChanged,
}

#[derive(Debug)]
pub enum ServiceCommand {
    StartCapture(Arc<Mutex<RecordingBuffer>>),
    StartCaptureChecked(
        Arc<Mutex<RecordingBuffer>>,
        std::sync::mpsc::Sender<Result<u64, String>>,
    ),
    StopCapture,
    StopCaptureChecked(std::sync::mpsc::Sender<Result<(), String>>),
    InitializeHotkeys(
        crate::capture::Hotkeys,
        std::sync::mpsc::Sender<Result<(), String>>,
    ),
    SetHotkeys(
        crate::capture::Hotkeys,
        std::sync::mpsc::Sender<Result<(), String>>,
    ),
    ArmExecution(bool, std::sync::mpsc::Sender<Result<(), String>>),
    Shutdown,
}

#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::{
    InputServiceHandle, current_layout_klid, execution_ready, gesture_settings, init_process_dpi,
    make_driver, snapshot_geometry, start_input_service,
};

#[cfg(not(windows))]
pub mod unsupported;

#[cfg(not(windows))]
pub use unsupported::{
    InputServiceHandle, current_layout_klid, execution_ready, gesture_settings, init_process_dpi,
    make_driver, snapshot_geometry, start_input_service,
};
