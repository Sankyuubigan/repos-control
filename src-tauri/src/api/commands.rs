use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager, State};

use crate::domain::contracts::{
    CommitDraftStore, CommitMessageProvider, ConfigStore, GitApi, GitWriteApi,
};
use crate::domain::project::{Project, ProjectStatus};
use crate::domain::usecases;
use crate::infra::{
    FileCommitDraftStore, FileConfigStore, Git2Write, GixGit, StubCommitProvider, WatcherManager,
};

const STATUS_COMMAND_TIMEOUT: Duration = Duration::from_secs(40);
const STATUS_SLOTS: usize = 4;

pub struct Services {
    pub config: Arc<dyn ConfigStore>,
    pub git: Arc<dyn GitApi>,
    pub write_git: Arc<dyn GitWriteApi>,
    pub provider: Arc<dyn CommitMessageProvider>,
    pub draft_store: Arc<dyn CommitDraftStore>,
    pub busy: Arc<AtomicBool>,
    pub status_slots: Arc<tokio::sync::Semaphore>,
}

impl Services {
    pub fn new() -> Result<Self, String> {
        let config = FileConfigStore::new().map_err(|err| format!("config init: {err:#}"))?;
        let draft_store =
            FileCommitDraftStore::new().map_err(|err| format!("draft store init: {err:#}"))?;
        Ok(Self {
            config: Arc::new(config),
            git: Arc::new(GixGit::new()),
            write_git: Arc::new(Git2Write),
            provider: Arc::new(StubCommitProvider),
            draft_store: Arc::new(draft_store),
            busy: Arc::new(AtomicBool::new(false)),
            status_slots: Arc::new(tokio::sync::Semaphore::new(STATUS_SLOTS)),
        })
    }
}

fn refresh_watcher(app: &AppHandle, services: &Services) {
    let paths: Vec<std::path::PathBuf> = usecases::list_projects(&*services.config)
        .into_iter()
        .map(|p| p.path)
        .collect();
    app.state::<WatcherManager>().set_project_paths(&paths);
}

#[tauri::command]
pub fn list_projects(services: State<'_, Services>) -> Vec<Project> {
    usecases::list_projects(&*services.config)
}

#[tauri::command]
pub fn add_project(
    app: AppHandle,
    services: State<'_, Services>,
    path: String,
) -> Result<(), String> {
    usecases::add_project(&*services.config, Path::new(&path))?;
    refresh_watcher(&app, &services);
    Ok(())
}

#[tauri::command]
pub fn remove_project(
    app: AppHandle,
    services: State<'_, Services>,
    path: String,
) -> Result<(), String> {
    usecases::remove_project(&*services.config, Path::new(&path))?;
    refresh_watcher(&app, &services);
    Ok(())
}

#[tauri::command]
pub fn reorder_projects(
    app: AppHandle,
    services: State<'_, Services>,
    paths: Vec<String>,
) -> Result<(), String> {
    let paths: Vec<std::path::PathBuf> = paths.into_iter().map(std::path::PathBuf::from).collect();
    usecases::reorder_projects(&*services.config, &paths)?;
    refresh_watcher(&app, &services);
    Ok(())
}

#[tauri::command]
pub async fn get_project_status(
    services: State<'_, Services>,
    project_path: String,
) -> Result<ProjectStatus, String> {
    let _permit = services
        .status_slots
        .acquire()
        .await
        .map_err(|err| format!("Ошибка слота статуса: {err}"))?;
    let git = Arc::clone(&services.inner().git);
    let what = project_path.clone();
    let task = tauri::async_runtime::spawn_blocking(move || {
        usecases::get_project_status(git.as_ref(), Path::new(&project_path))
    });
    match tokio::time::timeout(STATUS_COMMAND_TIMEOUT, task).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(err)) => Err(format!("Ошибка фоновой задачи: {err}")),
        Err(_) => {
            log::error!("get_project_status {what}: превышен внешний таймаут");
            Err("Таймаут получения статуса".to_string())
        }
    }
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

async fn run_write(
    busy: Arc<AtomicBool>,
    write_git: Arc<dyn GitWriteApi>,
    project_path: String,
    action: impl FnOnce(&dyn GitWriteApi, &Path) -> Result<(), String> + Send + 'static,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        busy.store(true, Ordering::SeqCst);
        let result = action(write_git.as_ref(), Path::new(&project_path));
        busy.store(false, Ordering::SeqCst);
        result
    })
    .await
    .map_err(|err| format!("Ошибка фоновой задачи: {err}"))?
}

#[tauri::command]
pub async fn stage_files(
    services: State<'_, Services>,
    project_path: String,
    paths: Vec<String>,
) -> Result<(), String> {
    let write_git = Arc::clone(&services.inner().write_git);
    let busy = Arc::clone(&services.inner().busy);
    run_write(busy, write_git, project_path, move |git, path| {
        usecases::stage_files(git, path, paths.clone())
    })
    .await
}

#[tauri::command]
pub async fn unstage_files(
    services: State<'_, Services>,
    project_path: String,
    paths: Vec<String>,
) -> Result<(), String> {
    let write_git = Arc::clone(&services.inner().write_git);
    let busy = Arc::clone(&services.inner().busy);
    run_write(busy, write_git, project_path, move |git, path| {
        usecases::unstage_files(git, path, paths.clone())
    })
    .await
}

#[tauri::command]
pub async fn discard_files(
    services: State<'_, Services>,
    project_path: String,
    paths: Vec<String>,
) -> Result<(), String> {
    let write_git = Arc::clone(&services.inner().write_git);
    let busy = Arc::clone(&services.inner().busy);
    run_write(busy, write_git, project_path, move |git, path| {
        usecases::discard_files(git, path, paths.clone())
    })
    .await
}

#[tauri::command]
pub async fn commit_changes(
    services: State<'_, Services>,
    project_path: String,
    message: String,
) -> Result<(), String> {
    let write_git = Arc::clone(&services.inner().write_git);
    let draft_store = Arc::clone(&services.inner().draft_store);
    let busy = Arc::clone(&services.inner().busy);
    tauri::async_runtime::spawn_blocking(move || {
        busy.store(true, Ordering::SeqCst);
        let result = usecases::commit_changes(
            write_git.as_ref(),
            draft_store.as_ref(),
            Path::new(&project_path),
            &message,
        );
        busy.store(false, Ordering::SeqCst);
        result
    })
    .await
    .map_err(|err| format!("Ошибка фоновой задачи: {err}"))?
}

#[tauri::command]
pub async fn push_changes(
    services: State<'_, Services>,
    project_path: String,
) -> Result<(), String> {
    let write_git = Arc::clone(&services.inner().write_git);
    let busy = Arc::clone(&services.inner().busy);
    run_write(busy, write_git, project_path, |git, path| {
        usecases::push_changes(git, Path::new(path))
    })
    .await
}

#[tauri::command]
pub async fn read_commit_message(
    services: State<'_, Services>,
    project_path: String,
) -> Result<String, String> {
    let draft_store = Arc::clone(&services.inner().draft_store);
    tauri::async_runtime::spawn_blocking(move || {
        Ok(usecases::read_commit_message(
            draft_store.as_ref(),
            Path::new(&project_path),
        ))
    })
    .await
    .map_err(|err| format!("Ошибка фоновой задачи: {err}"))?
}

#[tauri::command]
pub async fn write_commit_message(
    services: State<'_, Services>,
    project_path: String,
    message: String,
) -> Result<(), String> {
    let draft_store = Arc::clone(&services.inner().draft_store);
    tauri::async_runtime::spawn_blocking(move || {
        usecases::write_commit_message(draft_store.as_ref(), Path::new(&project_path), &message)
    })
    .await
    .map_err(|err| format!("Ошибка фоновой задачи: {err}"))?
}