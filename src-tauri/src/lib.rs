//! Repeat 桌面应用入口。
pub mod capture;
mod commands;
pub mod document;
pub mod event;
pub mod geometry;
mod overlay;
pub mod platform;
pub mod scheduler;
mod session;
pub mod storage;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use tauri::Manager;
pub fn run() {
    platform::init_process_dpi();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|window, event| {
            if window.label() == "main"
                && let tauri::WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                window.app_handle().exit(0);
            }
        })
        .setup(|app| {
            let folder = std::env::var_os("REPEAT_DATA_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or(app.path().app_data_dir()?);
            std::fs::create_dir_all(&folder)?;
            let session = session::AppSession::new(&folder.join("tasks.sqlite3"))?;
            // Register the session before creating auxiliary WebViews, which can
            // pump window messages and allow the main WebView to invoke commands.
            app.manage(session.clone());
            session.init(app.handle())?;
            overlay::create(app.handle())?;
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
        .run({
            let shutdown_state = Arc::new(AtomicU8::new(0));
            move |app, event| {
                if let tauri::RunEvent::ExitRequested { api, code, .. } = event
                    && shutdown_state.load(Ordering::Acquire) != 2
                    && let Some(session) = app.try_state::<Arc<session::AppSession>>()
                {
                    // The executor can need the UI thread to hide its windows.
                    // Do not wait for it while running the UI event callback.
                    api.prevent_exit();
                    if shutdown_state
                        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                    {
                        let session = Arc::clone(&session);
                        let app = app.clone();
                        let shutdown_state = shutdown_state.clone();
                        std::thread::spawn(move || {
                            session.shutdown(&app);
                            shutdown_state.store(2, Ordering::Release);
                            app.exit(code.unwrap_or(0));
                        });
                    }
                }
            }
        });
}
