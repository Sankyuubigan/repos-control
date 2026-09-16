use std::path::Path;

use crate::domain::commit_prompt;
use crate::domain::contracts::{CommitMessageProvider, ConfigStore, GitApi};
use crate::domain::project::{Project, ProjectStatus};

pub fn list_projects<C: ConfigStore + ?Sized>(store: &C) -> Vec<Project> {
    store.list_projects()
}

pub fn get_project_status<G: GitApi + ?Sized>(git: &G, project_path: &Path) -> ProjectStatus {
    match git.status(project_path) {
        Ok(status) => status,
        Err(err) => {
            log::warn!("status failed for {}: {err:#}", project_path.display());
            ProjectStatus {
                is_repo: false,
                error: Some(err.to_string()),
                ..ProjectStatus::default()
            }
        }
    }
}

pub fn generate_commit_message<G: GitApi + ?Sized, P: CommitMessageProvider + ?Sized>(
    git: &G,
    provider: &P,
    project_path: &Path,
    notes: &str,
) -> Result<String, String> {
    let diff = git.collect_diff(project_path, true).map_err(|err| {
        log::error!("collect_diff failed: {err:#}");
        format!("Не удалось собрать diff: {err}")
    })?;
    if diff.trim().is_empty() {
        return Err("Нет изменений для коммита".to_string());
    }
    let prompt = commit_prompt::build_prompt(notes, &diff);
    provider.generate(&prompt).map_err(|err| {
        log::error!("commit message generation failed: {err:#}");
        format!("Ошибка генерации: {err}")
    })
}

pub fn add_project<C: ConfigStore + ?Sized>(store: &C, path: &Path) -> Result<(), String> {
    if !path.is_dir() {
        log::error!("add_project: not a directory: {}", path.display());
        return Err("Указанный путь не является папкой".to_string());
    }
    store.add_project(path.to_path_buf()).map_err(|err| {
        log::error!("add_project failed: {err:#}");
        format!("Не удалось сохранить проект: {err}")
    })
}

pub fn remove_project<C: ConfigStore + ?Sized>(store: &C, path: &Path) -> Result<(), String> {
    store.remove_project(path).map_err(|err| {
        log::error!("remove_project failed: {err:#}");
        format!("Не удалось удалить проект: {err}")
    })
}