use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Context;

use crate::domain::contracts::CommitDraftStore;

pub struct FileCommitDraftStore {
    path: PathBuf,
    inner: Mutex<HashMap<String, String>>,
}

impl FileCommitDraftStore {
    pub fn new() -> Result<Self, anyhow::Error> {
        let path = default_drafts_path()?;
        let drafts = load_drafts(&path)?;
        Ok(Self {
            path,
            inner: Mutex::new(drafts),
        })
    }

    fn key(project_path: &Path) -> String {
        project_path.to_string_lossy().to_string()
    }

    fn save(&self, drafts: &HashMap<String, String>) -> Result<(), anyhow::Error> {
        let text = serde_json::to_string_pretty(drafts).context("serialize drafts")?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        fs::write(&self.path, text).with_context(|| format!("write {}", self.path.display()))?;
        Ok(())
    }
}

impl CommitDraftStore for FileCommitDraftStore {
    fn read_draft(&self, project_path: &Path) -> Result<String, anyhow::Error> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let k = Self::key(project_path);
        Ok(guard.get(&k).cloned().unwrap_or_default())
    }

    fn write_draft(&self, project_path: &Path, message: &str) -> Result<(), anyhow::Error> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let k = Self::key(project_path);
        if message.is_empty() {
            guard.remove(&k);
        } else {
            guard.insert(k, message.to_string());
        }
        let snapshot = guard.clone();
        drop(guard);
        self.save(&snapshot)
    }

    fn clear_draft(&self, project_path: &Path) -> Result<(), anyhow::Error> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let k = Self::key(project_path);
        if guard.remove(&k).is_some() {
            let snapshot = guard.clone();
            drop(guard);
            self.save(&snapshot)?;
        }
        Ok(())
    }
}

fn default_drafts_path() -> Result<PathBuf, anyhow::Error> {
    let base = std::env::var("APPDATA").context("APPDATA is not set")?;
    Ok(PathBuf::from(base)
        .join("com.reposcontrol.app")
        .join("commit_drafts.json"))
}

fn load_drafts(path: &Path) -> Result<HashMap<String, String>, anyhow::Error> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let drafts: HashMap<String, String> =
        serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(drafts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("commit_drafts_test_{ts}"))
    }

    #[test]
    fn read_write_clear_roundtrip() {
        let dir = temp_dir();
        let path = dir.join("commit_drafts.json");
        let store = FileCommitDraftStore {
            path: path.clone(),
            inner: Mutex::new(HashMap::new()),
        };

        let repo = Path::new("D:\\projects\\repo1");
        assert_eq!(store.read_draft(repo).unwrap(), "");

        store.write_draft(repo, "feat: my commit").unwrap();
        assert_eq!(store.read_draft(repo).unwrap(), "feat: my commit");

        let loaded = load_drafts(&path).unwrap();
        assert_eq!(
            loaded.get(&FileCommitDraftStore::key(repo)).unwrap(),
            "feat: my commit"
        );

        store.clear_draft(repo).unwrap();
        assert_eq!(store.read_draft(repo).unwrap(), "");
        let loaded = load_drafts(&path).unwrap();
        assert!(!loaded.contains_key(&FileCommitDraftStore::key(repo)));

        let _ = fs::remove_dir_all(&dir);
    }
}
