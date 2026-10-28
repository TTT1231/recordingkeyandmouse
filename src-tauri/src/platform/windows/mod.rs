//! Windows 平台层：DPI 声明、显示器几何、键盘布局名与注入来源标记。

pub(crate) mod click_highlight;
pub mod driver;
mod environment;
pub mod input_service;

use windows::Win32::Foundation::LPARAM;
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor,
    MDT_EFFECTIVE_DPI, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayout, GetKeyboardLayoutNameW};
use windows::core::BOOL;

use crate::geometry::{DesktopGeometry, MonitorInfo};

/// 引擎注入事件的 dwExtraInfo 标记：用于在钩子里区分本工具注入的输入。
/// 录制/执行互斥已保证钩子不会录到自己；该标记是第二道保险。
pub const REPEAT_INJECT_MARKER: usize = 0x0052_4550_4541_5401;

/// 声明 Per-Monitor V2 DPI 感知：钩子与注入坐标统一为物理像素。
/// 失败（如清单已设置或系统过旧）不阻塞启动；坐标差异需在原生环境核对。
pub fn init_process_dpi() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT(
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2.0,
        ));
    }
}

unsafe extern "system" fn enum_monitor_proc(
    hmon: HMONITOR,
    _hdc: windows::Win32::Graphics::Gdi::HDC,
    lprc: *mut windows::Win32::Foundation::RECT,
    ldata: LPARAM,
) -> BOOL {
    unsafe {
        let list = &mut *(ldata.0 as *mut Vec<(HMONITOR, windows::Win32::Foundation::RECT)>);
        if let Some(rc) = lprc.as_ref() {
            list.push((hmon, *rc));
        }
        BOOL(1)
    }
}

/// 枚举显示器：名称、物理矩形与有效 DPI。
pub fn snapshot_geometry() -> DesktopGeometry {
    let mut raw: Vec<(HMONITOR, windows::Win32::Foundation::RECT)> = Vec::new();
    let ctx = LPARAM(&mut raw as *mut _ as isize);
    let enumerated =
        unsafe { EnumDisplayMonitors(None, None, Some(enum_monitor_proc), ctx).as_bool() };
    if !enumerated {
        return DesktopGeometry::default();
    }
    let mut monitors = Vec::with_capacity(raw.len());
    for (hmon, _) in raw {
        // 用 GetMonitorInfoW 取物理矩形与设备名（进程已声明 PMv2，不再被虚拟化）。
        let info = unsafe {
            let mut ex = MONITORINFOEXW::default();
            ex.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            let pi = &mut ex as *mut MONITORINFOEXW as *mut MONITORINFO;
            if !GetMonitorInfoW(hmon, pi).as_bool() {
                continue;
            }
            ex
        };
        let rc = info.monitorInfo.rcMonitor;
        let mut dpi_x = 0u32;
        let mut dpi_y = 0u32;
        unsafe {
            let _ = GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        }
        let name_len = info.szDevice.iter().position(|c| *c == 0).unwrap_or(0);
        monitors.push(MonitorInfo {
            name: String::from_utf16_lossy(&info.szDevice[..name_len]),
            x: rc.left,
            y: rc.top,
            width: rc.right - rc.left,
            height: rc.bottom - rc.top,
            dpi: dpi_x,
        });
    }
    DesktopGeometry { monitors }
}

/// 当前线程键盘布局 KLID（如 "00000804"）。仅作为录制环境信息记录。
pub fn current_layout_klid() -> String {
    unsafe {
        let _ = GetKeyboardLayout(0);
        let mut buf = [0u16; 9];
        if GetKeyboardLayoutNameW(&mut buf).is_ok() {
            let len = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
            return String::from_utf16_lossy(&buf[..len]);
        }
        "unknown".into()
    }
}

/// 构造原生回放驱动（供会话服务调用；签名与占位后端一致）。
pub fn make_driver(
    stop: &std::sync::Arc<crate::scheduler::StopSignal>,
    geometry: &DesktopGeometry,
) -> Result<driver::NativeDriver, String> {
    driver::NativeDriver::new(stop, geometry)
}

pub use input_service::InputServiceHandle;
pub use input_service::start_input_service;

/// 执行按录制内容原样回放，不检查前台应用；倒计时结束后只检查用户已有按住状态，
/// 避免物理按键与回放输入混叠。
pub fn execution_ready() -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    unsafe {
        for vk in 1..=254 {
            if GetAsyncKeyState(vk) < 0 {
                return Err("开始执行前请松开当前按住的键和鼠标，再重试".into());
            }
        }
    }
    Ok(())
}

pub fn gesture_settings() -> crate::document::GestureSettings {
    use windows::Win32::UI::{
        Input::KeyboardAndMouse::GetDoubleClickTime,
        WindowsAndMessaging::{GetSystemMetrics, SM_CXDOUBLECLK, SM_CYDOUBLECLK},
    };
    unsafe {
        crate::document::GestureSettings {
            double_click_us: i64::from(GetDoubleClickTime()) * 1000,
            radius_x: (GetSystemMetrics(SM_CXDOUBLECLK) / 2).max(1),
            radius_y: (GetSystemMetrics(SM_CYDOUBLECLK) / 2).max(1),
        }
    }
}
