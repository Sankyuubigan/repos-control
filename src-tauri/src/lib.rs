mod api;
mod domain;
mod infra;

use std::net::TcpListener;
use std::sync::Mutex;

use api::Services;
use tauri::Manager;

const READINESS_PORT: u16 = 14262;

#[allow(dead_code)] // держит порт занятым на всё время жизни приложения
struct ReadinessPort(Mutex<Option<TcpListener>>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let services = Services::new().expect("init application services");

    tauri::Builder::default()
        .plugin(tauri_plugin_logs::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(services)
        .setup(|app| {
            match TcpListener::bind(("127.0.0.1", READINESS_PORT)) {
                Ok(listener) => {
                    app.manage(ReadinessPort(Mutex::new(Some(listener))));
                    log::info!("readiness port {READINESS_PORT} bound");
                }
                Err(err) => {
                    log::warn!("readiness port {READINESS_PORT} already in use: {err}");
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api::commands::list_projects,
            api::commands::add_project,
            api::commands::remove_project,
            api::commands::get_project_status,
            api::commands::generate_commit_message,
            api::commands::pick_project_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}