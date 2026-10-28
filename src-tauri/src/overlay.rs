//! 执行状态条：始终穿透鼠标，停止区域由原生输入钩子识别。
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "replay-overlay";

// Tauri's Windows show() can activate an initially hidden window even when it
// was built non-focusable. Showing these visual-only windows must never move
// keyboard focus away from the application being recorded or controlled.
#[cfg(windows)]
fn show_without_activation(window: &tauri::WebviewWindow) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowPos,
    };
    let hwnd =
        windows::Win32::Foundation::HWND(window.hwnd().map_err(|error| error.to_string())?.0);
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        )
        .map_err(|error| error.to_string())
    }
}

#[cfg(not(windows))]
fn show_without_activation(window: &tauri::WebviewWindow) -> Result<(), String> {
    window.show().map_err(|error| error.to_string())
}

#[cfg(windows)]
fn hide_visual(window: &tauri::WebviewWindow) {
    use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};
    if let Ok(hwnd) = window.hwnd() {
        unsafe {
            let _ = ShowWindow(windows::Win32::Foundation::HWND(hwnd.0), SW_HIDE);
        }
    }
}

#[cfg(not(windows))]
fn hide_visual(window: &tauri::WebviewWindow) {
    let _ = window.hide();
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(
        app,
        LABEL,
        WebviewUrl::App("index.html?overlay=replay".into()),
    )
    .title("Repeat 执行状态")
    .inner_size(300.0, 52.0)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .focusable(false)
    .visible(false)
    .build()?;
    window.set_ignore_cursor_events(true)?;
    for (index, _) in crate::platform::snapshot_geometry()
        .monitors
        .iter()
        .enumerate()
    {
        {
            let border = WebviewWindowBuilder::new(
                app,
                format!("replay-border-{index}"),
                WebviewUrl::App("execution-frame.html".into()),
            )
            .title("Repeat 执行范围")
            .inner_size(1.0, 1.0)
            .resizable(false)
            .decorations(false)
            .shadow(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .focusable(false)
            .visible(false)
            .build()?;
            border.set_ignore_cursor_events(true)?;
        }
    }
    Ok(())
}

pub fn show(app: &AppHandle) -> Result<(), String> {
    show_status(app)?;
    if let Err(error) = show_borders(app) {
        hide(app);
        return Err(format!("无法显示执行状态提示：{error}"));
    }
    Ok(())
}

/// Recording uses the same bottom status control, but no execution frame.
pub fn show_recording(app: &AppHandle) -> Result<(), String> {
    show_status(app)
}

fn show_status(app: &AppHandle) -> Result<(), String> {
    let window = app.get_webview_window(LABEL).ok_or("执行状态条不可用")?;
    let main = app.get_webview_window("main").ok_or("主窗口不可用")?;
    let monitor = main
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("无法确定执行状态条的显示器")?;
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let width = (300.0 * scale).round() as i64;
    let height = (52.0 * scale).round() as i64;
    let area_width = i64::from(area.size.width);
    let area_height = i64::from(area.size.height);
    if area_width < width || area_height < height {
        return Err("显示器工作区太小，无法显示执行状态条".into());
    }
    let margin = ((20.0 * scale).round() as i64).min(area_height - height);
    let left = i64::from(area.position.x) + (area_width - width) / 2;
    let top = i64::from(area.position.y) + area_height - height - margin;
    window
        .set_position(PhysicalPosition::new(left as i32, top as i32))
        .map_err(|e| e.to_string())?;
    let origin = window.inner_position().map_err(|e| e.to_string())?;
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let x1 = (252.0 * scale).round() as i32;
    let y1 = (6.0 * scale).round() as i32;
    let x2 = (292.0 * scale).round() as i32;
    let y2 = (46.0 * scale).round() as i32;
    crate::platform::set_overlay_stop_region(Some(crate::platform::StopRegion {
        x: origin.x + x1,
        y: origin.y + y1,
        width: (x2 - x1) as u32,
        height: (y2 - y1) as u32,
    }));
    if let Err(error) = show_without_activation(&window) {
        hide(app);
        return Err(format!("无法显示执行状态提示：{error}"));
    }
    Ok(())
}

pub fn hide(app: &AppHandle) {
    crate::platform::set_overlay_stop_region(None);
    if let Some(window) = app.get_webview_window(LABEL) {
        hide_visual(&window);
    }
    for (label, window) in app.webview_windows() {
        if label.starts_with("replay-border-") {
            hide_visual(&window);
        }
    }
}

fn show_borders(app: &AppHandle) -> Result<(), String> {
    let geometry = crate::platform::snapshot_geometry();
    if geometry.monitors.is_empty() {
        return Err("无法读取显示器范围".into());
    }
    if geometry.monitors.iter().enumerate().any(|(index, _)| {
        app.get_webview_window(&format!("replay-border-{index}"))
            .is_none()
    }) {
        return Err("显示器数量发生变化，请重启 Repeat".into());
    }
    for (index, monitor) in geometry.monitors.iter().enumerate() {
        if monitor.width <= 0 || monitor.height <= 0 {
            return Err("显示器范围无效".into());
        }
        let window = app
            .get_webview_window(&format!("replay-border-{index}"))
            .ok_or("显示器数量发生变化，请重启 Repeat")?;
        window
            .set_size(PhysicalSize::new(
                monitor.width as u32,
                monitor.height as u32,
            ))
            .map_err(|e| e.to_string())?;
        window
            .set_position(PhysicalPosition::new(monitor.x, monitor.y))
            .map_err(|e| e.to_string())?;
        // Align the painted client area with the screen, not the invisible
        // non-client margins Windows may add to an undecorated window.
        let outer = window.outer_position().map_err(|e| e.to_string())?;
        let inner = window.inner_position().map_err(|e| e.to_string())?;
        window
            .set_position(PhysicalPosition::new(
                monitor.x - (inner.x - outer.x),
                monitor.y - (inner.y - outer.y),
            ))
            .map_err(|e| e.to_string())?;
        show_without_activation(&window)?;
    }
    Ok(())
}
