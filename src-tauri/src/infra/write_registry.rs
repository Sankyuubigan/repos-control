use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Реестр проектов, в которых сейчас идёт запись (stage/unstage/discard/commit/push).
///
/// Нужен, чтобы фоновое авто-обновление статуса не читало репозиторий в момент
/// записи. Важно: блокировка **по пути**, а не глобальная — запись в одном
/// репозитории не должна останавливать обновление остальных.
pub struct WriteRegistry {
    paths: Mutex<HashSet<PathBuf>>,
}

impl WriteRegistry {
    pub fn new() -> Self {
        Self {
            paths: Mutex::new(HashSet::new()),
        }
    }

    /// Помечает проект как изменяемый. Снимается автоматически при drop.
    pub fn begin(&self, project_path: &Path) -> WriteGuard<'_> {
        if let Ok(mut paths) = self.paths.lock() {
            paths.insert(project_path.to_path_buf());
        }
        WriteGuard {
            registry: self,
            path: project_path.to_path_buf(),
        }
    }

    pub fn is_writing(&self, project_path: &Path) -> bool {
        self.paths
            .lock()
            .map(|paths| paths.contains(project_path))
            .unwrap_or(false)
    }
}

impl Default for WriteRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct WriteGuard<'a> {
    registry: &'a WriteRegistry,
    path: PathBuf,
}

impl Drop for WriteGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut paths) = self.registry.paths.lock() {
            paths.remove(&self.path);
        }
    }
}
