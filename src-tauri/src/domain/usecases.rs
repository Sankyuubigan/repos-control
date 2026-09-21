use std::path::{Path, PathBuf};

use crate::domain::commit_prompt;
use crate::domain::contracts::{
    CommitDraftStore, CommitMessageProvider, ConfigStore, GitApi, GitWriteApi,
};
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

pub fn reorder_projects<C: ConfigStore + ?Sized>(
    store: &C,
    paths: &[PathBuf],
) -> Result<(), String> {
    store.reorder_projects(paths).map_err(|err| {
        log::error!("reorder_projects failed: {err:#}");
        format!("Не удалось изменить порядок проектов: {err}")
    })
}

fn write_error(label: &str, project_path: &Path, err: anyhow::Error) -> String {
    log::error!("{label} failed for {}: {err:#}", project_path.display());
    format!("{label}: {err}")
}

pub fn stage_files<W: GitWriteApi + ?Sized>(
    git: &W,
    project_path: &Path,
    paths: Vec<String>,
) -> Result<(), String> {
    git.stage(project_path, &paths)
        .map_err(|err| write_error("Не удалось проиндексировать", project_path, err))
}

pub fn unstage_files<W: GitWriteApi + ?Sized>(
    git: &W,
    project_path: &Path,
    paths: Vec<String>,
) -> Result<(), String> {
    git.unstage(project_path, &paths)
        .map_err(|err| write_error("Не удалось снять с индекса", project_path, err))
}

pub fn discard_files<W: GitWriteApi + ?Sized>(
    git: &W,
    project_path: &Path,
    paths: Vec<String>,
) -> Result<(), String> {
    git.discard(project_path, &paths)
        .map_err(|err| write_error("Не удалось откатить изменения", project_path, err))
}

pub fn commit_changes<W: GitWriteApi + ?Sized, D: CommitDraftStore + ?Sized>(
    git: &W,
    drafts: &D,
    project_path: &Path,
    message: &str,
) -> Result<(), String> {
    if message.trim().is_empty() {
        return Err("Сообщение коммита пустое".to_string());
    }
    git.commit(project_path, message)
        .map_err(|err| write_error("Не удалось создать коммит", project_path, err))?;
    if let Err(err) = drafts.clear_draft(project_path) {
        log::warn!("Не удалось очистить черновик {}: {err:#}", project_path.display());
    }
    Ok(())
}

pub fn push_changes<W: GitWriteApi + ?Sized>(
    git: &W,
    project_path: &Path,
) -> Result<(), String> {
    git.push(project_path)
        .map_err(|err| write_error("Не удалось выполнить push", project_path, err))
}

pub fn read_commit_message<D: CommitDraftStore + ?Sized>(
    drafts: &D,
    project_path: &Path,
) -> String {
    match drafts.read_draft(project_path) {
        Ok(text) => text,
        Err(err) => {
            log::warn!(
                "read_draft failed for {}: {err:#}",
                project_path.display()
            );
            String::new()
        }
    }
}

pub fn write_commit_message<D: CommitDraftStore + ?Sized>(
    drafts: &D,
    project_path: &Path,
    message: &str,
) -> Result<(), String> {
    drafts
        .write_draft(project_path, message)
        .map_err(|err| write_error("Не удалось сохранить черновик", project_path, err))
}