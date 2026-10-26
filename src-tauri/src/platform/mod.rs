//! 平台层：把当前实际用到的 Windows 捕获、注入、坐标和停止操作隔离在小模块里（M1-02）。
//! 不依赖界面组件或数据库；测试用可替换的时钟/注入驱动（见 `scheduler::ReplayDriver`）。
//! Linux X11 后端在 M6 接入；此前非 Windows 平台使用明确报错的占位实现。

use std::sync::{Arc, Mutex};

use crate::capture::RecordingBuffer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyAction {
    ToggleRecording,
    StopExecution,
    EnvironmentChanged,
}

#[derive(Debug)]
pub enum ServiceCommand {
    StartCapture(Arc<Mutex<RecordingBuffer>>),
    StartCaptureChecked(
        Arc<Mutex<RecordingBuffer>>,
        std::sync::mpsc::Sender<Result<(), String>>,
    ),
    StopCapture,
    StopCaptureChecked(std::sync::mpsc::Sender<Result<(), String>>),
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
