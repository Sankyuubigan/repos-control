use std::path::Path;
use std::sync::Arc;

use tauri::State;

use crate::domain::commit_prompt;
use crate::domain::contracts::DiffScope;
use crate::domain::usecases;

use super::commands::Services;

#[tauri::command]
pub async fn get_commit_diff(
    services: State<'_, Services>,
    project_path: String,
    notes: String,
    lang: String,
    scope: String,
) -> Result<Vec<commit_prompt::ChatMessage>, String> {
    let scope = parse_scope(&scope)?;
    let git = Arc::clone(&services.inner().git);
    tauri::async_runtime::spawn_blocking(move || {
        usecases::get_commit_diff(git.as_ref(), Path::new(&project_path), &notes, &lang, scope)
    })
    .await
    .map_err(|err| format!("Ошибка фоновой задачи: {err}"))?
}

fn parse_scope(raw: &str) -> Result<DiffScope, String> {
    match raw {
        "staged" => Ok(DiffScope::Staged),
        "unstaged" => Ok(DiffScope::Unstaged),
        "all" => Ok(DiffScope::All),
        other => Err(format!("Неизвестный scope диффа: {other}")),
    }
}

#[tauri::command]
pub fn get_commit_model(services: State<'_, Services>) -> Option<String> {
    services.config.commit_model()
}

#[tauri::command]
pub fn set_commit_model(
    services: State<'_, Services>,
    model: Option<String>,
) -> Result<(), String> {
    services
        .config
        .set_commit_model(model)
        .map_err(|err| format!("Не удалось сохранить модель: {err}"))
}

#[tauri::command]
pub fn get_commit_lang(services: State<'_, Services>) -> Option<String> {
    services.config.commit_lang()
}

#[tauri::command]
pub fn set_commit_lang(
    services: State<'_, Services>,
    lang: Option<String>,
) -> Result<(), String> {
    services
        .config
        .set_commit_lang(lang)
        .map_err(|err| format!("Не удалось сохранить язык: {err}"))
}
