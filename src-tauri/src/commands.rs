//! 具名控制命令；输入热路径和执行序列均留在 Rust。
//! Commands run off the UI thread: status polling must not block native window
//! operations while another thread owns the session lock.
use crate::document::EditRequest;
use crate::session::{AppSession, ExecutionReportBundle, SessionSnapshot, WorkspaceView};
use crate::storage::Hotkeys;
use std::sync::Arc;
use tauri::{AppHandle, State};
type Session<'a> = State<'a, Arc<AppSession>>;
#[tauri::command]
pub async fn session_status(session: Session<'_>) -> Result<SessionSnapshot, String> {
    Ok(session.snapshot())
}
#[tauri::command]
pub async fn get_workspace(session: Session<'_>) -> Result<WorkspaceView, String> {
    session.workspace()
}
#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    session: Session<'_>,
    task_id: String,
    revision: u64,
) -> Result<SessionSnapshot, String> {
    session.start_recording(&app, &task_id, revision)?;
    Ok(session.snapshot())
}
#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    session: Session<'_>,
    gesture: Option<String>,
) -> Result<WorkspaceView, String> {
    session.stop_recording(&app, gesture.as_deref())?;
    session.workspace()
}
#[tauri::command]
pub async fn cancel_recording(
    app: AppHandle,
    session: Session<'_>,
    task_id: String,
    revision: u64,
) -> Result<WorkspaceView, String> {
    session.cancel_recording(&app, &task_id, revision)?;
    session.workspace()
}
#[tauri::command]
pub async fn retry_recording(
    app: AppHandle,
    session: Session<'_>,
    discard: bool,
) -> Result<WorkspaceView, String> {
    session.retry_recording(&app, discard)?;
    session.workspace()
}
#[tauri::command]
pub async fn create_task(
    app: AppHandle,
    session: Session<'_>,
    name: String,
) -> Result<WorkspaceView, String> {
    session.create_task(&app, name)?;
    session.workspace()
}
#[tauri::command]
pub async fn select_task(
    app: AppHandle,
    session: Session<'_>,
    task_id: String,
) -> Result<WorkspaceView, String> {
    session.select_task(&app, &task_id)?;
    session.workspace()
}
#[tauri::command]
pub async fn rename_task(
    app: AppHandle,
    session: Session<'_>,
    task_id: String,
    revision: u64,
    name: String,
) -> Result<WorkspaceView, String> {
    session.rename_task(&app, &task_id, revision, name)?;
    session.workspace()
}
#[tauri::command]
pub async fn delete_task(
    app: AppHandle,
    session: Session<'_>,
    task_id: String,
    revision: u64,
) -> Result<WorkspaceView, String> {
    session.delete_task(&app, &task_id, revision)?;
    session.workspace()
}
#[tauri::command]
pub async fn edit_task(
    app: AppHandle,
    session: Session<'_>,
    task_id: String,
    revision: u64,
    edit: EditRequest,
) -> Result<WorkspaceView, String> {
    session.edit(&app, &task_id, revision, edit)?;
    session.workspace()
}
#[tauri::command]
pub async fn task_history(
    app: AppHandle,
    session: Session<'_>,
    task_id: String,
    revision: u64,
    redo: bool,
) -> Result<WorkspaceView, String> {
    session.history(&app, &task_id, revision, redo)?;
    session.workspace()
}
#[tauri::command]
pub async fn set_hotkeys(
    app: AppHandle,
    session: Session<'_>,
    hotkeys: Hotkeys,
) -> Result<WorkspaceView, String> {
    session.set_hotkeys(&app, hotkeys)?;
    session.workspace()
}
#[tauri::command]
pub async fn demo_mode(
    app: AppHandle,
    session: Session<'_>,
    enter: bool,
) -> Result<WorkspaceView, String> {
    session.demo(&app, enter)?;
    session.workspace()
}
#[tauri::command]
pub async fn start_execution(
    app: AppHandle,
    session: Session<'_>,
    task_id: String,
    revision: u64,
    loop_count: u32,
    speed: f64,
) -> Result<SessionSnapshot, String> {
    session.start_execution(&app, &task_id, revision, loop_count, speed)?;
    Ok(session.snapshot())
}
#[tauri::command]
pub async fn stop_execution(
    app: AppHandle,
    session: Session<'_>,
) -> Result<SessionSnapshot, String> {
    session.stop_execution(&app)?;
    Ok(session.snapshot())
}
#[tauri::command]
pub async fn get_execution_report(
    session: Session<'_>,
) -> Result<Option<ExecutionReportBundle>, String> {
    Ok(session.report())
}
#[tauri::command]
pub async fn clear_error(app: AppHandle, session: Session<'_>) -> Result<(), String> {
    session.clear_error(&app);
    Ok(())
}
