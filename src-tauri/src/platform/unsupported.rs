//! 未实现平台保留明确错误，不伪装为可用后端。
use crate::{
    capture::RecordingBuffer,
    geometry::DesktopGeometry,
    platform::{HotkeyAction, ServiceCommand},
    scheduler::{PacerOutcome, ReplayDriver},
};
use std::sync::Arc;
pub struct InputServiceHandle;
impl InputServiceHandle {
    pub fn send_cmd(&self, _command: ServiceCommand) -> Result<(), String> {
        Err("当前平台输入后端尚未提供".into())
    }
    pub fn start_capture(
        &self,
        _buffer: Arc<std::sync::Mutex<RecordingBuffer>>,
    ) -> Result<u64, String> {
        Err("当前平台不能录制系统输入".into())
    }
    pub fn stop_capture(&self) -> Result<(), String> {
        Err("当前平台不能录制系统输入".into())
    }
    pub fn set_hotkeys(&self, _hotkeys: crate::capture::Hotkeys) -> Result<(), String> {
        Err("当前平台不能注册全局快捷键".into())
    }
    pub fn initialize_hotkeys(&self, _hotkeys: crate::capture::Hotkeys) -> Result<(), String> {
        Err("当前平台不能注册全局快捷键".into())
    }
    pub fn record_hotkey_available(&self) -> bool {
        false
    }
    pub fn stop_hotkey_available(&self) -> bool {
        false
    }
    pub fn registered_hotkey_vks(&self, _hotkeys: &crate::capture::Hotkeys) -> Vec<u32> {
        Vec::new()
    }
    pub fn arm_execution(&self, _active: bool) -> Result<(), String> {
        Err("当前平台未提供停止监听".into())
    }
    pub fn prepare_recording(
        &self,
        _preparation: Option<super::RecordingPreparation>,
    ) -> Result<(), String> {
        Err("当前平台不能录制系统输入".into())
    }
    pub fn shutdown(&self) {}
    pub fn status_lines(&self) -> Vec<String> {
        vec!["当前平台输入后端未验证".into()]
    }
}
pub fn start_input_service(
    _on_hotkey: Arc<dyn Fn(HotkeyAction) + Send + Sync>,
) -> Result<InputServiceHandle, String> {
    Err("当前平台输入后端未提供".into())
}
pub fn init_process_dpi() {}
pub fn snapshot_geometry() -> DesktopGeometry {
    DesktopGeometry::default()
}
pub fn current_layout_klid() -> String {
    "unknown".into()
}
pub fn execution_ready() -> Result<(), String> {
    Err("当前平台不能执行系统输入".into())
}
pub fn make_driver(
    _stop: &Arc<crate::scheduler::StopSignal>,
    _geometry: &DesktopGeometry,
) -> Result<UnsupportedDriver, String> {
    Err("当前平台不能执行系统输入".into())
}
pub struct UnsupportedDriver;
impl ReplayDriver for UnsupportedDriver {
    fn now(&mut self) -> std::time::Instant {
        std::time::Instant::now()
    }
    fn wait_until(&mut self, _deadline: std::time::Instant) -> PacerOutcome {
        PacerOutcome::Cancelled
    }
    fn inject(&mut self, _kind: &crate::event::EventKind) -> Result<(), String> {
        Err("当前平台不能执行系统输入".into())
    }
    fn release_held(&mut self) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }
}

pub fn gesture_settings() -> crate::document::GestureSettings {
    Default::default()
}
