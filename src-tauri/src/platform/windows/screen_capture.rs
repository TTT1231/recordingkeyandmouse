use super::{ScreenCaptureInfo, ScreenCaptureStatistics};
use crate::geometry::{DesktopGeometry, VirtualScreen};
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::core::{GUID, PCWSTR};

const FPS: u32 = 30;
const FRAME_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / FPS as u64);

fn elapsed_us(start: Instant) -> u64 {
    start.elapsed().as_micros().min(u64::MAX as u128) as u64
}

fn thread_cpu_us() -> Option<u64> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetCurrentThread, GetThreadTimes};
    let (mut creation, mut exit, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe {
        GetThreadTimes(
            GetCurrentThread(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
        .ok()?;
    }
    let ticks =
        |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    Some((ticks(kernel) + ticks(user)) / 10)
}

enum Command {
    Start(Instant, mpsc::SyncSender<Result<(), String>>),
    Finish(i64),
    Abort,
}

/// An initialized encoder waiting for the shared recording clock. No screen
/// pixels are acquired before `start`; all COM/GDI objects belong to the worker.
pub struct ScreenCapture {
    commands: mpsc::Sender<Command>,
    worker: Option<JoinHandle<Result<ScreenCaptureInfo, String>>>,
    error: Arc<Mutex<Option<String>>>,
}

impl ScreenCapture {
    pub fn prepare(path: &Path, geometry: &DesktopGeometry) -> Result<Self, String> {
        let rect = checked_bounds(geometry)?;
        if path.exists() {
            return Err("录制视频文件已经存在，未覆盖原文件".into());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("无法创建录制目录：{e}"))?;
        }
        let output = path.to_owned();
        let temporary = output.with_extension("part.mp4");
        // Reserve a unique path. Never truncate a previous recording or staging file.
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| format!("无法创建录制视频：{e}"))?;
        let (commands, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let error = Arc::new(Mutex::new(None));
        let worker_error = error.clone();
        let cleanup_path = temporary.clone();
        let worker = std::thread::Builder::new()
            .name("repeat-screen-video".into())
            .spawn(move || {
                let result = capture_worker(&temporary, &output, rect, rx, ready_tx);
                if let Err(reason) = &result {
                    *worker_error.lock().unwrap() = Some(reason.clone());
                }
                // Finalization moves the staged file; failures and cancellation
                // remove only this worker's own uniquely reserved temporary file.
                let _ = std::fs::remove_file(&temporary);
                result
            });
        let worker = match worker {
            Ok(worker) => worker,
            Err(reason) => {
                let _ = std::fs::remove_file(cleanup_path);
                return Err(format!("无法启动屏幕录制：{reason}"));
            }
        };
        let recording = Self {
            commands,
            worker: Some(worker),
            error,
        };
        match ready_rx.recv_timeout(Duration::from_secs(20)) {
            Ok(Ok(())) => Ok(recording),
            Ok(Err(reason)) => Err(reason),
            Err(_) => Err("屏幕编码器初始化超时或已退出".into()),
        }
    }

    pub fn start(&self, start: Instant) -> Result<(), String> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.commands
            .send(Command::Start(start, tx))
            .map_err(|_| self.failure())?;
        rx.recv_timeout(Duration::from_secs(5))
            .map_err(|_| self.failure())?
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }

    pub fn finish(mut self, duration_us: i64) -> Result<ScreenCaptureInfo, String> {
        let _ = self.commands.send(Command::Finish(duration_us));
        self.worker
            .take()
            .ok_or("屏幕录制已结束")?
            .join()
            .map_err(|_| "屏幕录制线程异常退出".to_string())?
    }

    fn failure(&self) -> String {
        self.error()
            .unwrap_or_else(|| "屏幕录制线程没有响应".into())
    }
}

impl Drop for ScreenCapture {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = self.commands.send(Command::Abort);
            let _ = worker.join();
        }
    }
}

fn checked_bounds(geometry: &DesktopGeometry) -> Result<VirtualScreen, String> {
    let mut left = i64::MAX;
    let mut top = i64::MAX;
    let mut right = i64::MIN;
    let mut bottom = i64::MIN;
    for monitor in &geometry.monitors {
        if monitor.width <= 0 || monitor.height <= 0 {
            return Err("显示器尺寸无效，无法录制画面".into());
        }
        left = left.min(i64::from(monitor.x));
        top = top.min(i64::from(monitor.y));
        right = right.max(i64::from(monitor.x) + i64::from(monitor.width));
        bottom = bottom.max(i64::from(monitor.y) + i64::from(monitor.height));
    }
    if geometry.monitors.is_empty() || right - left > 32768 || bottom - top > 32768 {
        return Err("桌面范围为空或过大，无法录制画面".into());
    }
    Ok(VirtualScreen {
        x: left as i32,
        y: top as i32,
        width: (right - left) as i32,
        height: (bottom - top) as i32,
    })
}

fn encoded_size(rect: VirtualScreen) -> (u32, u32) {
    let scale = (1920.0 / f64::from(rect.width))
        .min(1080.0 / f64::from(rect.height))
        .min(1.0);
    let even = |value: i32| (((f64::from(value) * scale).floor() as u32) & !1).max(2);
    (even(rect.width), even(rect.height))
}

fn capture_worker(
    temporary: &Path,
    output: &Path,
    rect: VirtualScreen,
    commands: mpsc::Receiver<Command>,
    ready: mpsc::SyncSender<Result<(), String>>,
) -> Result<ScreenCaptureInfo, String> {
    let initialized: Result<_, String> = (|| {
        let runtime = MediaRuntime::new()?;
        let (width, height) = encoded_size(rect);
        let capture = DesktopCapture::new(rect, width, height)?;
        let encoder = Encoder::new(temporary, width, height)?;
        Ok((runtime, capture, encoder))
    })();
    let (_runtime, mut capture, mut encoder) = match initialized {
        Ok(parts) => {
            let _ = ready.send(Ok(()));
            parts
        }
        Err(reason) => {
            let _ = ready.send(Err(reason.clone()));
            return Err(reason);
        }
    };
    let (start, started) = match commands.recv() {
        Ok(Command::Start(start, reply)) => (start, reply),
        _ => return Err("屏幕录制已取消".into()),
    };
    let cpu_start = thread_cpu_us();
    let mut write_total_us = 0;
    let mut write_max_us = 0;
    let mut max_frame_gap_us = 0;
    let mut pending = Vec::new();
    let mut incoming = Vec::new();
    match capture.frame(&mut pending) {
        Ok(()) => {
            let _ = started.send(Ok(()));
        }
        Err(reason) => {
            let _ = started.send(Err(reason.clone()));
            return Err(reason);
        }
    }
    let mut pending_us = 0;
    let mut frame_count = 0;
    let mut next_frame = start + FRAME_INTERVAL;
    let requested_duration_us = loop {
        // Drain a queued stop before considering a frame whose deadline has
        // already passed, including after a slow encoder call.
        let command = match commands.try_recv() {
            Ok(command) => Ok(command),
            Err(mpsc::TryRecvError::Disconnected) => Err(mpsc::RecvTimeoutError::Disconnected),
            Err(mpsc::TryRecvError::Empty) => {
                commands.recv_timeout(next_frame.saturating_duration_since(Instant::now()))
            }
        };
        match command {
            Ok(Command::Finish(duration)) => break duration,
            Ok(Command::Abort) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("屏幕录制已取消".into());
            }
            Ok(Command::Start(_, reply)) => {
                let _ = reply.send(Err("屏幕录制已经开始".into()));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let sampled_us = start.elapsed().as_micros().min(i64::MAX as u128) as i64;
                capture.frame(&mut incoming)?;
                // Finishing can arrive while a frame is copied/converted. Check
                // before committing the preceding sample's end timestamp.
                match commands.try_recv() {
                    Ok(Command::Finish(duration)) => break duration,
                    Ok(Command::Abort) | Err(mpsc::TryRecvError::Disconnected) => {
                        return Err("屏幕录制已取消".into());
                    }
                    Ok(Command::Start(_, reply)) => {
                        let _ = reply.send(Err("屏幕录制已经开始".into()));
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
                if sampled_us > pending_us {
                    let writing = Instant::now();
                    encoder.write(&pending, pending_us, sampled_us - pending_us)?;
                    let elapsed = elapsed_us(writing);
                    write_total_us += elapsed;
                    write_max_us = write_max_us.max(elapsed);
                    max_frame_gap_us = max_frame_gap_us.max((sampled_us - pending_us) as u64);
                    frame_count += 1;
                    std::mem::swap(&mut pending, &mut incoming);
                    pending_us = sampled_us;
                }
                // Skip missed samples instead of accumulating or catching up.
                next_frame += FRAME_INTERVAL;
                if next_frame <= Instant::now() {
                    let elapsed = start.elapsed().as_nanos();
                    let tick = elapsed / FRAME_INTERVAL.as_nanos() + 1;
                    next_frame =
                        start + FRAME_INTERVAL.saturating_mul(tick.min(u32::MAX as u128) as u32);
                }
            }
        }
    };
    if requested_duration_us < 0 {
        return Err("屏幕录制结束时间无效".into());
    }
    // Input stopping and video finalization are separate native operations.
    // A frame may have been encoded in between; retain its actual timestamp
    // instead of failing an otherwise complete recording. The document maps
    // only through its own input duration and never exposes this short tail.
    let duration_us =
        requested_duration_us.max(pending_us.checked_add(1).ok_or("视频时间超出范围")?);
    let writing = Instant::now();
    encoder.write(&pending, pending_us, duration_us - pending_us)?;
    let elapsed = elapsed_us(writing);
    write_total_us += elapsed;
    write_max_us = write_max_us.max(elapsed);
    frame_count += 1;
    let finalizing = Instant::now();
    encoder.finish()?;
    let finalize_us = elapsed_us(finalizing);
    drop(encoder);
    std::fs::OpenOptions::new()
        .write(true)
        .open(temporary)
        .and_then(|file| file.sync_all())
        .map_err(|e| format!("无法写入录制视频：{e}"))?;
    std::fs::rename(temporary, output).map_err(|e| format!("无法保存录制视频：{e}"))?;
    Ok(ScreenCaptureInfo {
        width: capture.width,
        height: capture.height,
        origin_x: rect.x,
        origin_y: rect.y,
        capture_width: rect.width as u32,
        capture_height: rect.height as u32,
        duration_us,
        frame_count,
        statistics: ScreenCaptureStatistics {
            write_total_us,
            write_max_us,
            max_frame_gap_us,
            finalize_us,
            worker_cpu_us: cpu_start
                .zip(thread_cpu_us())
                .map(|(before, after)| after.saturating_sub(before)),
            ..capture.statistics.clone()
        },
    })
}

struct MediaRuntime;
impl MediaRuntime {
    fn new() -> Result<Self, String> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(native_error)?;
            if let Err(error) = MFStartup(MF_VERSION, MFSTARTUP_FULL) {
                CoUninitialize();
                return Err(native_error(error));
            }
        }
        Ok(Self)
    }
}
impl Drop for MediaRuntime {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            CoUninitialize();
        }
    }
}

fn native_error(error: windows::core::Error) -> String {
    format!("Windows 屏幕录制失败：{error}")
}

struct DesktopCapture {
    desktop: HDC,
    memory: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    pixels: *mut u8,
    rect: VirtualScreen,
    width: u32,
    height: u32,
    statistics: ScreenCaptureStatistics,
}
impl DesktopCapture {
    fn new(rect: VirtualScreen, width: u32, height: u32) -> Result<Self, String> {
        let mut capture = Self {
            desktop: HDC::default(),
            memory: HDC::default(),
            bitmap: HBITMAP::default(),
            previous: HGDIOBJ::default(),
            pixels: std::ptr::null_mut(),
            rect,
            width,
            height,
            statistics: ScreenCaptureStatistics::default(),
        };
        unsafe {
            capture.desktop = GetDC(None);
            if capture.desktop.is_invalid() {
                return Err("无法读取当前桌面".into());
            }
            capture.memory = CreateCompatibleDC(Some(capture.desktop));
            if capture.memory.is_invalid() {
                return Err("无法创建屏幕采集缓冲".into());
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut pixels = std::ptr::null_mut();
            capture.bitmap = CreateDIBSection(
                Some(capture.desktop),
                &info,
                DIB_RGB_COLORS,
                &mut pixels,
                None,
                0,
            )
            .map_err(native_error)?;
            capture.pixels = pixels.cast();
            capture.previous = SelectObject(capture.memory, capture.bitmap.into());
            if capture.previous.is_invalid() || capture.pixels.is_null() {
                return Err("屏幕采集缓冲不可用".into());
            }
            SetStretchBltMode(capture.memory, HALFTONE);
        }
        Ok(capture)
    }

    fn frame(&mut self, frame: &mut Vec<u8>) -> Result<(), String> {
        let captured = Instant::now();
        unsafe {
            let copied =
                if self.width as i32 == self.rect.width && self.height as i32 == self.rect.height {
                    BitBlt(
                        self.memory,
                        0,
                        0,
                        self.rect.width,
                        self.rect.height,
                        Some(self.desktop),
                        self.rect.x,
                        self.rect.y,
                        SRCCOPY | CAPTUREBLT,
                    )
                    .is_ok()
                } else {
                    StretchBlt(
                        self.memory,
                        0,
                        0,
                        self.width as i32,
                        self.height as i32,
                        Some(self.desktop),
                        self.rect.x,
                        self.rect.y,
                        self.rect.width,
                        self.rect.height,
                        SRCCOPY | CAPTUREBLT,
                    )
                    .as_bool()
                };
            if !copied {
                return Err("无法采集当前桌面画面，原录制未被替换".into());
            }
            if !GdiFlush().as_bool() {
                return Err("无法完成桌面画面采集".into());
            }
            let length = (self.width * self.height * 4) as usize;
            frame.resize(length, 0);
            std::ptr::copy_nonoverlapping(self.pixels, frame.as_mut_ptr(), length);
            let capture_us = elapsed_us(captured);
            self.statistics.captured_frames += 1;
            self.statistics.capture_total_us += capture_us;
            self.statistics.capture_max_us = self.statistics.capture_max_us.max(capture_us);
            Ok(())
        }
    }
}
impl Drop for DesktopCapture {
    fn drop(&mut self) {
        unsafe {
            if !self.previous.is_invalid() && !self.memory.is_invalid() {
                SelectObject(self.memory, self.previous);
            }
            if !self.bitmap.is_invalid() {
                let _ = DeleteObject(self.bitmap.into());
            }
            if !self.memory.is_invalid() {
                let _ = DeleteDC(self.memory);
            }
            if !self.desktop.is_invalid() {
                ReleaseDC(None, self.desktop);
            }
        }
    }
}

struct Encoder {
    writer: IMFSinkWriter,
    stream: u32,
}
impl Encoder {
    fn new(path: &Path, width: u32, height: u32) -> Result<Self, String> {
        use std::os::windows::ffi::OsStrExt;
        let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 3).map_err(native_error)?;
            let attributes = attributes.ok_or("无法创建编码参数")?;
            attributes
                .SetUINT32(&MF_LOW_LATENCY, 1)
                .map_err(native_error)?;
            attributes
                .SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)
                .map_err(native_error)?;
            let writer = MFCreateSinkWriterFromURL(PCWSTR(path.as_ptr()), None, &attributes)
                .map_err(native_error)?;
            let output = media_type(&MFVideoFormat_H264, width, height)?;
            output
                .SetUINT32(&MF_MT_AVG_BITRATE, 6_000_000)
                .map_err(native_error)?;
            output
                .SetUINT32(&MF_MT_MPEG2_PROFILE, 66)
                .map_err(native_error)?;
            let stream = writer.AddStream(&output).map_err(native_error)?;
            let input = media_type(&MFVideoFormat_RGB32, width, height)?;
            input
                .SetUINT32(&MF_MT_DEFAULT_STRIDE, width * 4)
                .map_err(native_error)?;
            writer
                .SetInputMediaType(stream, &input, None)
                .map_err(native_error)?;
            writer.BeginWriting().map_err(native_error)?;
            Ok(Self { writer, stream })
        }
    }
    fn write(&mut self, pixels: &[u8], time_us: i64, duration_us: i64) -> Result<(), String> {
        let time = time_us.checked_mul(10).ok_or("视频时间超出范围")?;
        let duration = duration_us
            .checked_mul(10)
            .filter(|v| *v > 0)
            .ok_or("视频帧时长无效")?;
        unsafe {
            let buffer = MFCreateMemoryBuffer(pixels.len() as u32).map_err(native_error)?;
            let mut destination = std::ptr::null_mut();
            buffer
                .Lock(&mut destination, None, None)
                .map_err(native_error)?;
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), destination, pixels.len());
            buffer.Unlock().map_err(native_error)?;
            buffer
                .SetCurrentLength(pixels.len() as u32)
                .map_err(native_error)?;
            let sample = MFCreateSample().map_err(native_error)?;
            sample.AddBuffer(&buffer).map_err(native_error)?;
            sample.SetSampleTime(time).map_err(native_error)?;
            sample.SetSampleDuration(duration).map_err(native_error)?;
            self.writer
                .WriteSample(self.stream, &sample)
                .map_err(native_error)
        }
    }
    fn finish(&mut self) -> Result<(), String> {
        unsafe { self.writer.Finalize().map_err(native_error) }
    }
}

fn media_type(format: &GUID, width: u32, height: u32) -> Result<IMFMediaType, String> {
    unsafe {
        let media = MFCreateMediaType().map_err(native_error)?;
        media
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(native_error)?;
        media
            .SetGUID(&MF_MT_SUBTYPE, format)
            .map_err(native_error)?;
        media
            .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            .map_err(native_error)?;
        media
            .SetUINT64(&MF_MT_FRAME_SIZE, ((width as u64) << 32) | height as u64)
            .map_err(native_error)?;
        media
            .SetUINT64(&MF_MT_FRAME_RATE, ((FPS as u64) << 32) | 1)
            .map_err(native_error)?;
        media
            .SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)
            .map_err(native_error)?;
        media
            .SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT601.0 as u32)
            .map_err(native_error)?;
        media
            .SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)
            .map_err(native_error)?;
        Ok(media)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::MonitorInfo;

    #[test]
    fn bounds_keep_negative_origin_and_encode_even_sized_desktop() {
        let geometry = DesktopGeometry {
            monitors: vec![MonitorInfo {
                name: "test".into(),
                x: -1920,
                y: 0,
                width: 3840,
                height: 1080,
                dpi: 96,
            }],
        };
        let rect = checked_bounds(&geometry).unwrap();
        assert_eq!(
            (rect.x, rect.y, rect.width, rect.height),
            (-1920, 0, 3840, 1080)
        );
        assert_eq!(encoded_size(rect), (1920, 540));
        assert!(checked_bounds(&DesktopGeometry::default()).is_err());
    }

    #[test]
    #[ignore = "录制真实桌面画面约一秒到隔离 target 文件，仅在交互桌面运行"]
    fn native_screen_capture_creates_local_mp4() {
        crate::platform::init_process_dpi();
        let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("screen-video-{}", crate::document::new_id()));
        let path = directory.join("capture.mp4");
        let recording =
            ScreenCapture::prepare(&path, &crate::platform::snapshot_geometry()).unwrap();
        assert!(!path.exists());
        let start = Instant::now();
        recording.start(start).unwrap();
        std::thread::sleep(Duration::from_millis(1100));
        let duration = start.elapsed().as_micros() as i64;
        let info = recording.finish(duration).unwrap();
        assert!(info.duration_us >= duration && info.duration_us - duration < 1_000_000);
        assert!(info.frame_count >= 2);
        assert!(std::fs::metadata(&path).unwrap().len() > 1000);
        assert!(!path.with_extension("part.mp4").exists());
        std::fs::write(
            directory.join("capture-evidence.json"),
            serde_json::to_vec_pretty(&info).unwrap(),
        )
        .unwrap();
        println!("screen capture test video: {}", path.display());
    }

    #[test]
    #[ignore = "录制真实桌面五秒并输出聚合性能计数，需交互桌面"]
    fn native_screen_capture_performance() {
        fn process_cpu_us() -> u64 {
            use windows::Win32::{
                Foundation::FILETIME,
                System::Threading::{GetCurrentProcess, GetProcessTimes},
            };
            let (mut creation, mut exit, mut kernel, mut user) = (
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
            );
            unsafe {
                GetProcessTimes(
                    GetCurrentProcess(),
                    &mut creation,
                    &mut exit,
                    &mut kernel,
                    &mut user,
                )
                .unwrap();
            }
            let ticks = |time: FILETIME| {
                (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
            };
            (ticks(kernel) + ticks(user)) / 10
        }
        crate::platform::init_process_dpi();
        let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("screen-performance-{}", crate::document::new_id()));
        let path = directory.join("capture.mp4");
        let capture = ScreenCapture::prepare(&path, &crate::platform::snapshot_geometry()).unwrap();
        let cpu_start = process_cpu_us();
        let start = Instant::now();
        capture.start(start).unwrap();
        std::thread::sleep(Duration::from_secs(5));
        let info = capture.finish(start.elapsed().as_micros() as i64).unwrap();
        assert!(info.frame_count > 1);
        let evidence = serde_json::json!({"video":path,"capture":info,"processCpuUs":process_cpu_us()-cpu_start,"wallUs":elapsed_us(start),"fileBytes":std::fs::metadata(&path).unwrap().len()});
        std::fs::write(
            directory.join("performance.json"),
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();
        println!("SCREEN_PERFORMANCE {evidence}");
    }
}
