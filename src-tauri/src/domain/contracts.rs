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