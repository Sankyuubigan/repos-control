mod api;
mod domain;
mod infra;

use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use api::Services;
use infra::WatcherManager;
use tauri::Manager;

const READINESS_PORT: u16 = 14262;

#[allow(dead_code)] // держит порт занятым на всё время жизни приложения
struct ReadinessPort(Mutex<Option<TcpListener>>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri_plugin_llama_engine::engine::config::set_app_data_dir_name("com.reposcontrol.app");
    tauri_plugin_cloud_routers::set_app_data_dir_name("com.reposcontrol.app");
    let services = Services::new().expect("init application services");

    tauri::Builder::default()
        .plugin(tauri_plugin_logs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_llama_engine::init())
        .plugin(tauri_plugin_cloud_routers::init())
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
            let services = app.state::<Services>();
            let paths: Vec<PathBuf> = services
                .config
                .list_projects()
                .into_iter()
                .map(|p| p.path)
                .collect();
            match WatcherManager::new(
                app.handle().clone(),
                Arc::clone(&services.status),
                Arc::clone(&services.writes),
            ) {
                Ok(watcher) => {
                    watcher.set_project_paths(&paths);
                    app.manage(watcher);
                }
                Err(err) => {
                    log::warn!("fs-watcher disabled: {err}");
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api::commands::list_projects,
            api::commands::add_project,
            api::commands::remove_project,
            api::commands::reorder_projects,
            api::commands::get_project_status,
            api::settings_commands::get_commit_diff,
            api::settings_commands::get_commit_model,
            api::settings_commands::set_commit_model,
            api::settings_commands::get_commit_lang,
            api::settings_commands::set_commit_lang,
            api::commands::pick_project_folder,
            api::commands::stage_files,
            api::commands::unstage_files,
            api::commands::discard_files,
            api::commands::commit_changes,
            api::commands::push_changes,
            api::commands::read_commit_message,
            api::commands::write_commit_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}