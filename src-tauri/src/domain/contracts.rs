use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffScope {
    Staged,
    Unstaged,
    All,
}

pub trait CommitDraftStore: Send + Sync {
    fn read_draft(&self, project_path: &Path) -> Result<String, anyhow::Error>;
    fn write_draft(&self, project_path: &Path, message: &str) -> Result<(), anyhow::Error>;
    fn clear_draft(&self, project_path: &Path) -> Result<(), anyhow::Error>;
}

use crate::domain::project::{Project, ProjectStatus};

pub trait ConfigStore: Send + Sync {
    fn list_projects(&self) -> Vec<Project>;
    fn add_project(&self, path: PathBuf) -> Result<(), anyhow::Error>;
    fn remove_project(&self, path: &Path) -> Result<(), anyhow::Error>;
    fn reorder_projects(&self, paths: &[PathBuf]) -> Result<(), anyhow::Error>;
    fn commit_model(&self) -> Option<String>;
    fn set_commit_model(&self, model: Option<String>) -> Result<(), anyhow::Error>;
    fn commit_lang(&self) -> Option<String>;
    fn set_commit_lang(&self, lang: Option<String>) -> Result<(), anyhow::Error>;
}

pub trait GitApi: Send + Sync {
    fn status(&self, project_path: &Path) -> Result<ProjectStatus, anyhow::Error>;
    fn collect_diff(&self, project_path: &Path, scope: DiffScope) -> Result<String, anyhow::Error>;
}

pub trait GitWriteApi: Send + Sync {
    fn stage(&self, project_path: &Path, paths: &[String]) -> Result<(), anyhow::Error>;
    fn unstage(&self, project_path: &Path, paths: &[String]) -> Result<(), anyhow::Error>;
    fn discard(&self, project_path: &Path, paths: &[String]) -> Result<(), anyhow::Error>;
    fn commit(&self, project_path: &Path, message: &str) -> Result<(), anyhow::Error>;
    fn push(&self, project_path: &Path) -> Result<(), anyhow::Error>;
}