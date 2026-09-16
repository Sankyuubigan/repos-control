use std::path::Path;
use std::sync::Arc;

use tauri::State;

use crate::domain::contracts::{CommitMessageProvider, ConfigStore, GitApi};
use crate::domain::project::{Project, ProjectStatus};
use crate::domain::usecases;
use crate::infra::{FileConfigStore, GixGit, StubCommitProvider};

pub struct Services {
    pub config: Arc<dyn ConfigStore>,
    pub git: Arc<dyn GitApi>,
    pub provider: Arc<dyn CommitMessageProvider>,
}

impl Services {
    pub fn new() -> Result<Self, String> {
        let config = FileConfigStore::new().map_err(|err| format!("config init: {err:#}"))?;
        Ok(Self {
            config: Arc::new(config),
            git: Arc::new(GixGit),
            provider: Arc::new(StubCommitProvider),
        })
    }
}

#[tauri::command]
pub fn list_projects(services: State<'_, Services>) -> Vec<Project> {
    usecases::list_projects(&*services.config)
}

#[tauri::command]
pub fn add_project(services: State<'_, Services>, path: String) -> Result<(), String> {
    usecases::add_project(&*services.config, Path::new(&path))
}

#[tauri::command]
pub fn remove_project(services: State<'_, Services>, path: String) -> Result<(), String> {
    usecases::remove_project(&*services.config, Path::new(&path))
}

#[tauri::command]
pub async fn get_project_status(
    services: State<'_, Services>,
    project_path: String,
) -> Result<ProjectStatus, String> {
    let git = Arc::clone(&services.inner().git);
    tauri::async_runtime::spawn_blocking(move || {
        usecases::get_project_status(git.as_ref(), Path::new(&project_path))
    })
    .await
    .map_err(|err| format!("Ошибка фоновой задачи: {err}"))
}

#[tauri::command]
pub async fn generate_commit_message(
    services: State<'_, Services>,
    project_path: String,
    notes: String,
) -> Result<String, String> {
    let git = Arc::clone(&services.inner().git);
    let provider = Arc::clone(&services.inner().provider);
    tauri::async_runtime::spawn_blocking(move || {
        usecases::generate_commit_message(
            git.as_ref(),
            provider.as_ref(),
            Path::new(&project_path),
            &notes,
        )
    })
    .await
    .map_err(|err| format!("Ошибка фоновой задачи: {err}"))?
}

#[tauri::command]
pub async fn pick_project_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Option<String>, String> {
        use tauri_plugin_dialog::DialogExt;
        let picked = app.dialog().file().blocking_pick_folder();
        Ok(picked.map(|p| p.to_string()))
    })
    .await
    .map_err(|err| format!("Ошибка диалога: {err}"))?
}