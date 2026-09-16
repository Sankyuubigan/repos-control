#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    tauri_plugin_logs::early_init("repos_control.log");
    tauri_plugin_logs::early_log("INFO", "=== запуск Repos Control ===");
    repos_control_lib::run();
}