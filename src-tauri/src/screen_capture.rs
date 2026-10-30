//! Local screen video. Frames and encoding stay on a dedicated native worker.
#[cfg(not(windows))]
use crate::geometry::DesktopGeometry;
use serde::Serialize;
#[cfg(not(windows))]
use std::{path::Path, time::Instant};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenCaptureInfo {
    pub width: u32,
    pub height: u32,
    pub origin_x: i32,
    pub origin_y: i32,
    pub capture_width: u32,
    pub capture_height: u32,
    pub duration_us: i64,
    pub frame_count: u64,
    pub statistics: ScreenCaptureStatistics,
}

/// Aggregate timings only; no desktop pixels or input contents enter diagnostics.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenCaptureStatistics {
    pub captured_frames: u64,
    pub capture_total_us: u64,
    pub capture_max_us: u64,
    pub conversion_total_us: u64,
    pub conversion_max_us: u64,
    pub write_total_us: u64,
    pub write_max_us: u64,
    pub finalize_us: u64,
    pub max_frame_gap_us: u64,
    pub worker_cpu_us: Option<u64>,
}

#[cfg(windows)]
#[path = "platform/windows/screen_capture.rs"]
mod native;

#[cfg(windows)]
pub use native::ScreenCapture;

#[cfg(not(windows))]
pub struct ScreenCapture;

#[cfg(not(windows))]
impl ScreenCapture {
    pub fn prepare(_path: &Path, _geometry: &DesktopGeometry) -> Result<Self, String> {
        Err("当前平台尚不支持桌面画面录制".into())
    }
    pub fn start(&self, _start: Instant) -> Result<(), String> {
        Err("当前平台尚不支持桌面画面录制".into())
    }
    pub fn finish(self, _duration_us: i64) -> Result<ScreenCaptureInfo, String> {
        Err("当前平台尚不支持桌面画面录制".into())
    }
    pub fn error(&self) -> Option<String> {
        None
    }
}
