use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::domain::contracts::ConfigStore;
use crate::domain::project::Project;

#[derive(Debug, Serialize, Deserialize, Default)]
struct AppConfig {
    projects: Vec<Project>,
}

pub struct FileConfigStore {
    path: PathBuf,
    inner: Mutex<Vec<Project>>,
}

impl FileConfigStore {
    pub fn new() -> Result<Self, anyhow::Error> {
        let path = default_config_path()?;
        let projects = load_projects(&path)?;
        Ok(Self { path, inner: Mutex::new(projects) })
    }

    fn save(&self, projects: &[Project]) -> Result<(), anyhow::Error> {
        let cfg = AppConfig { projects: projects.to_vec() };
        let text = serde_json::to_string_pretty(&cfg).context("serialize config")?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        fs::write(&self.path, text).with_context(|| format!("write {}", self.path.display()))?;
        Ok(())
    }
}

impl ConfigStore for FileConfigStore {
    fn list_projects(&self) -> Vec<Project> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.clone()
    }

    fn add_project(&self, path: PathBuf) -> Result<(), anyhow::Error> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if !guard.iter().any(|p| p.path == path) {
            guard.push(Project { path });
        }
        let snapshot = guard.clone();
        drop(guard);
        self.save(&snapshot)
    }

    fn remove_project(&self, path: &Path) -> Result<(), anyhow::Error> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.retain(|p| p.path != path);
        let snapshot = guard.clone();
        drop(guard);
        self.save(&snapshot)
    }
}

fn default_config_path() -> Result<PathBuf, anyhow::Error> {
    let base = std::env::var("APPDATA").context("APPDATA is not set")?;
    Ok(PathBuf::from(base).join("com.reposcontrol.app").join("app_config.json"))
}

fn load_projects(path: &Path) -> Result<Vec<Project>, anyhow::Error> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let cfg: AppConfig = serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(cfg.projects)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("appcfg_test_{ts}"))
    }

    #[test]
    fn add_remove_list_roundtrip() {
        let dir = temp_dir();
        let path = dir.join("app_config.json");
        let store = FileConfigStore { path, inner: Mutex::new(Vec::new()) };

        store.add_project(PathBuf::from("D:\\a\\repo1")).unwrap();
        store.add_project(PathBuf::from("D:\\a\\repo2")).unwrap();
        store.add_project(PathBuf::from("D:\\a\\repo1")).unwrap(); // dup ignored
        assert_eq!(store.list_projects().len(), 2);

        let persisted = load_projects(&store.path).unwrap();
        assert_eq!(persisted.len(), 2);

        store.remove_project(Path::new(r"D:\a\repo1")).unwrap();
        assert_eq!(store.list_projects().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apis_available() {
        let _ = FileConfigStore::new();
        let _ = env::var("APPDATA");
    }
}