use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::domain::contracts::ConfigStore;
use crate::domain::project::Project;

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
struct AppConfig {
    #[serde(default)]
    projects: Vec<Project>,
    #[serde(default)]
    commit_model: Option<String>,
    #[serde(default)]
    commit_lang: Option<String>,
}

pub struct FileConfigStore {
    path: PathBuf,
    inner: Mutex<AppConfig>,
}

impl FileConfigStore {
    pub fn new() -> Result<Self, anyhow::Error> {
        let path = default_config_path()?;
        let config = load_config(&path)?;
        Ok(Self { path, inner: Mutex::new(config) })
    }

    /// Атомарный merge-сохранение: обновляем только поля projects/commit_model/commit_lang,
    /// остальные ключи файла (движковые, плагинные) не затираем.
    fn save(&self, config: &AppConfig) -> Result<(), anyhow::Error> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        ko_json_store::update_json(&self.path, |root: &mut serde_json::Value| {
            if !root.is_object() {
                *root = serde_json::json!({});
            }
            let obj = root.as_object_mut().expect("root is object");
            obj.insert(
                "projects".to_string(),
                serde_json::to_value(&config.projects).expect("serialize projects"),
            );
            match &config.commit_model {
                Some(m) => {
                    obj.insert("commit_model".to_string(), serde_json::json!(m));
                }
                None => {
                    obj.remove("commit_model");
                }
            }
            match &config.commit_lang {
                Some(l) => {
                    obj.insert("commit_lang".to_string(), serde_json::json!(l));
                }
                None => {
                    obj.remove("commit_lang");
                }
            }
        })
        .map_err(|e| anyhow::anyhow!("config merge save failed: {e}"))?;
        Ok(())
    }
}

impl ConfigStore for FileConfigStore {
    fn list_projects(&self) -> Vec<Project> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.projects.clone()
    }

    fn add_project(&self, path: PathBuf) -> Result<(), anyhow::Error> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if !guard.projects.iter().any(|p| p.path == path) {
            guard.projects.push(Project { path });
        }
        let snapshot = (*guard).clone();
        drop(guard);
        self.save(&snapshot)
    }

    fn remove_project(&self, path: &Path) -> Result<(), anyhow::Error> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.projects.retain(|p| p.path != path);
        let snapshot = (*guard).clone();
        drop(guard);
        self.save(&snapshot)
    }

    fn reorder_projects(&self, paths: &[PathBuf]) -> Result<(), anyhow::Error> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut reordered: Vec<Project> = Vec::with_capacity(guard.projects.len());
        for path in paths {
            if let Some(pos) = guard.projects.iter().position(|p| &p.path == path) {
                reordered.push(guard.projects.remove(pos));
            }
        }
        reordered.extend(guard.projects.drain(..));
        guard.projects = reordered;
        log::info!("reordered projects: {:?}", guard.projects);
        let snapshot = (*guard).clone();
        drop(guard);
        self.save(&snapshot)
    }

    fn commit_model(&self) -> Option<String> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.commit_model.clone()
    }

    fn set_commit_model(&self, model: Option<String>) -> Result<(), anyhow::Error> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.commit_model = model;
        let snapshot = (*guard).clone();
        drop(guard);
        self.save(&snapshot)
    }

    fn commit_lang(&self) -> Option<String> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.commit_lang.clone()
    }

    fn set_commit_lang(&self, lang: Option<String>) -> Result<(), anyhow::Error> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.commit_lang = lang;
        let snapshot = (*guard).clone();
        drop(guard);
        self.save(&snapshot)
    }
}

fn default_config_path() -> Result<PathBuf, anyhow::Error> {
    let base = std::env::var("APPDATA").context("APPDATA is not set")?;
    Ok(PathBuf::from(base).join("com.reposcontrol.app").join("app_config.json"))
}

fn load_config(path: &Path) -> Result<AppConfig, anyhow::Error> {
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let cfg: AppConfig = serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(cfg)
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
        let store = FileConfigStore { path, inner: Mutex::new(AppConfig::default()) };

        store.add_project(PathBuf::from("D:\\a\\repo1")).unwrap();
        store.add_project(PathBuf::from("D:\\a\\repo2")).unwrap();
        store.add_project(PathBuf::from("D:\\a\\repo1")).unwrap();
        assert_eq!(store.list_projects().len(), 2);

        let persisted = load_config(&store.path).unwrap();
        assert_eq!(persisted.projects.len(), 2);

        store.remove_project(Path::new(r"D:\a\repo1")).unwrap();
        assert_eq!(store.list_projects().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apis_available() {
        let _ = FileConfigStore::new();
        let _ = env::var("APPDATA");
    }

    #[test]
    fn reorder_projects_roundtrip() {
        let dir = temp_dir();
        let path = dir.join("app_config.json");
        let store = FileConfigStore { path, inner: Mutex::new(AppConfig::default()) };

        store.add_project(PathBuf::from("D:\\a\\repo1")).unwrap();
        store.add_project(PathBuf::from("D:\\a\\repo2")).unwrap();
        store.add_project(PathBuf::from("D:\\a\\repo3")).unwrap();

        store
            .reorder_projects(&[
                PathBuf::from("D:\\a\\repo3"),
                PathBuf::from("D:\\a\\repo1"),
                PathBuf::from("D:\\a\\repo2"),
            ])
            .unwrap();

        let persisted = load_config(&store.path).unwrap();
        let paths: Vec<_> = persisted.projects.iter().map(|p| p.path.clone()).collect();
        assert_eq!(
            paths,
            vec![
                PathBuf::from("D:\\a\\repo3"),
                PathBuf::from("D:\\a\\repo1"),
                PathBuf::from("D:\\a\\repo2"),
            ]
        );

        store
            .reorder_projects(&[PathBuf::from("D:\\a\\repo1")])
            .unwrap();
        let persisted = load_config(&store.path).unwrap();
        let paths: Vec<_> = persisted.projects.iter().map(|p| p.path.clone()).collect();
        assert_eq!(
            paths,
            vec![
                PathBuf::from("D:\\a\\repo1"),
                PathBuf::from("D:\\a\\repo3"),
                PathBuf::from("D:\\a\\repo2"),
            ]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn commit_model_roundtrip_and_merge_preserves_foreign_keys() {
        let dir = temp_dir();
        let path = dir.join("app_config.json");
        let store = FileConfigStore { path, inner: Mutex::new(AppConfig::default()) };

        store.set_commit_model(Some("llama:C:\\m.gguf".to_string())).unwrap();
        store.add_project(PathBuf::from("D:\\a\\repo1")).unwrap();

        let raw = fs::read_to_string(&store.path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["commit_model"], "llama:C:\\m.gguf");
        assert_eq!(value["projects"].as_array().unwrap().len(), 1);

        store.set_commit_model(None).unwrap();
        let raw = fs::read_to_string(&store.path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(value.get("commit_model").is_none());
        assert_eq!(value["projects"].as_array().unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
