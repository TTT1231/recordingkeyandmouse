//! Repeat 桌面应用入口。
pub mod capture;
mod commands;
pub mod document;
pub mod event;
pub mod geometry;
pub mod platform;
pub mod scheduler;
mod session;
pub mod storage;
use std::sync::Arc;
use tauri::Manager;
pub fn run() {
    platform::init_process_dpi();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let folder = std::env::var_os("REPEAT_DATA_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or(app.path().app_data_dir()?);
            std::fs::create_dir_all(&folder)?;
            let session = session::AppSession::new(&folder.join("tasks.sqlite3"))?;
            session.init(app.handle())?;
            app.manage(session);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::session_status,
            commands::get_workspace,
            commands::start_recording,
            commands::stop_recording,
            commands::cancel_recording,
            commands::retry_recording,
            commands::create_task,
            commands::select_task,
            commands::rename_task,
            commands::delete_task,
            commands::edit_task,
            commands::task_history,
            commands::set_hotkeys,
            commands::demo_mode,
            commands::start_execution,
            commands::stop_execution,
            commands::get_execution_report,
            commands::clear_error
        ])
        .build(tauri::generate_context!())
        .expect("无法启动 Repeat")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::ExitRequested { .. })
                && let Some(session) = app.try_state::<Arc<session::AppSession>>()
            {
                session.shutdown(app);
            }
        });
}
