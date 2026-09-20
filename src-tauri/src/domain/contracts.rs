use std::path::{Path, PathBuf};

use crate::domain::project::{Project, ProjectStatus};

pub trait ConfigStore: Send + Sync {
    fn list_projects(&self) -> Vec<Project>;
    fn add_project(&self, path: PathBuf) -> Result<(), anyhow::Error>;
    fn remove_project(&self, path: &Path) -> Result<(), anyhow::Error>;
}

pub trait GitApi: Send + Sync {
    fn status(&self, project_path: &Path) -> Result<ProjectStatus, anyhow::Error>;
    fn collect_diff(&self, project_path: &Path, staged_first: bool) -> Result<String, anyhow::Error>;
}

pub trait CommitMessageProvider: Send + Sync {
    fn generate(&self, prompt: &str) -> Result<String, anyhow::Error>;
}

pub trait GitWriteApi: Send + Sync {
    fn stage(&self, project_path: &Path, paths: &[String]) -> Result<(), anyhow::Error>;
    fn unstage(&self, project_path: &Path, paths: &[String]) -> Result<(), anyhow::Error>;
    fn discard(&self, project_path: &Path, paths: &[String]) -> Result<(), anyhow::Error>;
    fn commit(&self, project_path: &Path, message: &str) -> Result<(), anyhow::Error>;
    fn push(&self, project_path: &Path) -> Result<(), anyhow::Error>;
    fn read_commit_message(&self, project_path: &Path) -> Result<String, anyhow::Error>;
    fn write_commit_message(&self, project_path: &Path, message: &str) -> Result<(), anyhow::Error>;
}