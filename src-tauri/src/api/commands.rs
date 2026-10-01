use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::{AppHandle, Manager, State};

use crate::domain::contracts::{CommitDraftStore, ConfigStore, GitApi, GitWriteApi};
use crate::domain::project::Project;
use crate::domain::usecases;
use crate::infra::{
    FileCommitDraftStore, FileConfigStore, Git2Write, GixGit, StatusHub, StatusSnapshot,
    WatcherManager, WriteRegistry,
};

pub struct Services {
    pub config: Arc<dyn ConfigStore>,
    pub git: Arc<dyn GitApi>,
    pub write_git: Arc<dyn GitWriteApi>,
    pub draft_store: Arc<dyn CommitDraftStore>,
    pub writes: Arc<WriteRegistry>,
    pub status: Arc<StatusHub>,
}

impl Services {
    pub fn new() -> Result<Self, String> {
        let config = FileConfigStore::new().map_err(|err| format!("config init: {err:#}"))?;
        let draft_store =
            FileCommitDraftStore::new().map_err(|err| format!("draft store init: {err:#}"))?;
        let git: Arc<dyn GitApi> = Arc::new(GixGit::new());
        Ok(Self {
            config: Arc::new(config),
            git: Arc::clone(&git),
            write_git: Arc::new(Git2Write),
            draft_store: Arc::new(draft_store),
            writes: Arc::new(WriteRegistry::new()),
            status: Arc::new(StatusHub::new(git)),
        })
    }
}

fn refresh_watcher(app: &AppHandle, services: &Services) {
    let paths: Vec<PathBuf> = usecases::list_projects(&*services.config)
        .into_iter()
        .map(|p| p.path)
        .collect();
    match app.try_state::<WatcherManager>() {
        Some(watcher) => watcher.set_project_paths(&paths),
        None => log::warn!("refresh_watcher: watcher не инициализирован, пути не обновлены"),
    }
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
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    usecases::reorder_projects(&*services.config, &paths)?;
    refresh_watcher(&app, &services);
    Ok(())
}

#[tauri::command]
pub async fn get_project_status(
    services: State<'_, Services>,
    project_path: String,
) -> Result<StatusSnapshot, String> {
    services.status.read(Path::new(&project_path)).await
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

/// Общая часть всех команд записи: запись → **свежий** статус в том же ответе.
///
/// Раньше команда возвращала `()`, фронт делал второй запрос `get_project_status`,
/// и тот попадал в TTL-кэш — UI показывал состояние «до операции». Теперь ответ
/// содержит фактическое состояние сразу после записи (один round-trip, гонок нет).
async fn run_write(
    status: Arc<StatusHub>,
    writes: Arc<WriteRegistry>,
    write_git: Arc<dyn GitWriteApi>,
    project_path: String,
    action: impl FnOnce(&dyn GitWriteApi, &Path) -> Result<(), String> + Send + 'static,
) -> Result<StatusSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = writes.begin(Path::new(&project_path));
        action(write_git.as_ref(), Path::new(&project_path))?;
        Ok(status.read_blocking(Path::new(&project_path)))
    })
    .await
    .map_err(|err| format!("Ошибка фоновой задачи: {err}"))?
}

#[tauri::command]
pub async fn stage_files(
    services: State<'_, Services>,
    project_path: String,
    paths: Vec<String>,
) -> Result<StatusSnapshot, String> {
    let inner = services.inner();
    let write_git = Arc::clone(&inner.write_git);
    run_write(
        Arc::clone(&inner.status),
        Arc::clone(&inner.writes),
        write_git,
        project_path,
        move |git, path| usecases::stage_files(git, path, paths.clone()),
    )
    .await
}

#[tauri::command]
pub async fn unstage_files(
    services: State<'_, Services>,
    project_path: String,
    paths: Vec<String>,
) -> Result<StatusSnapshot, String> {
    let inner = services.inner();
    let write_git = Arc::clone(&inner.write_git);
    run_write(
        Arc::clone(&inner.status),
        Arc::clone(&inner.writes),
        write_git,
        project_path,
        move |git, path| usecases::unstage_files(git, path, paths.clone()),
    )
    .await
}

#[tauri::command]
pub async fn discard_files(
    services: State<'_, Services>,
    project_path: String,
    paths: Vec<String>,
) -> Result<StatusSnapshot, String> {
    let inner = services.inner();
    let write_git = Arc::clone(&inner.write_git);
    run_write(
        Arc::clone(&inner.status),
        Arc::clone(&inner.writes),
        write_git,
        project_path,
        move |git, path| usecases::discard_files(git, path, paths.clone()),
    )
    .await
}

#[tauri::command]
pub async fn commit_changes(
    services: State<'_, Services>,
    project_path: String,
    message: String,
) -> Result<StatusSnapshot, String> {
    let inner = services.inner();
    let write_git = Arc::clone(&inner.write_git);
    let draft_store = Arc::clone(&inner.draft_store);
    run_write(
        Arc::clone(&inner.status),
        Arc::clone(&inner.writes),
        write_git,
        project_path,
        move |git, path| usecases::commit_changes(git, draft_store.as_ref(), path, &message),
    )
    .await
}

#[tauri::command]
pub async fn push_changes(
    services: State<'_, Services>,
    project_path: String,
) -> Result<StatusSnapshot, String> {
    let inner = services.inner();
    let write_git = Arc::clone(&inner.write_git);
    run_write(
        Arc::clone(&inner.status),
        Arc::clone(&inner.writes),
        write_git,
        project_path,
        move |git, path| usecases::push_changes(git, path),
    )
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
