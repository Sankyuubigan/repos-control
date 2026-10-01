use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::domain::contracts::GitApi;
use crate::domain::project::ProjectStatus;
use crate::domain::usecases;

const STATUS_COMMAND_TIMEOUT: Duration = Duration::from_secs(40);
const STATUS_SLOTS: usize = 4;
const SLOT_POLL: Duration = Duration::from_millis(20);

/// Статус проекта вместе с порядковым номером чтения.
///
/// Чтения одного проекта выполняются строго последовательно (см. `ReadGate`),
/// поэтому `seq` совпадает и с порядком старта, и с порядком завершения. Фронт
/// применяет только чтение с максимальным `seq` — позднее завершившееся «старое»
/// чтение уже не может перерисовать более свежий статус (в git-расширении VS Code
/// то же самое делает CancellationTokenSource).
#[derive(Debug, Clone, Serialize)]
pub struct StatusSnapshot {
    pub path: String,
    pub seq: u64,
    #[serde(flatten)]
    pub status: ProjectStatus,
}

/// Части, нужные для одного чтения. Клонируются (все `Arc`), чтобы унестись в
/// поток `spawn_blocking` без передачи `&self`.
struct ReadCtx {
    git: Arc<dyn GitApi>,
    slots: Arc<Semaphore>,
    gate: Arc<ReadGate>,
    seq: Arc<AtomicU64>,
}

impl ReadCtx {
    fn read(&self, path: PathBuf) -> StatusSnapshot {
        // Порядок захвата всегда один и тот же: сначала гейт проекта, потом
        // глобальный слот. Иначе возможен взаимный deadlock.
        let _serial = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        let _permit = acquire_slot(&self.slots);
        let seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        StatusSnapshot {
            path: path.to_string_lossy().into_owned(),
            seq,
            status: usecases::get_project_status(self.git.as_ref(), &path),
        }
    }
}

type ReadGate = Mutex<()>;

fn acquire_slot(slots: &Arc<Semaphore>) -> OwnedSemaphorePermit {
    loop {
        match Arc::clone(slots).try_acquire_owned() {
            Ok(permit) => return permit,
            Err(_) => std::thread::sleep(SLOT_POLL),
        }
    }
}

/// Единственная точка чтения статуса проекта: Tauri-команды, фоновый watcher-а
/// и перечитывание сразу после операции записи.
pub struct StatusHub {
    git: Arc<dyn GitApi>,
    slots: Arc<Semaphore>,
    seq: Arc<AtomicU64>,
    gates: Mutex<HashMap<PathBuf, Arc<ReadGate>>>,
}

impl StatusHub {
    pub fn new(git: Arc<dyn GitApi>) -> Self {
        Self {
            git,
            slots: Arc::new(Semaphore::new(STATUS_SLOTS)),
            seq: Arc::new(AtomicU64::new(0)),
            gates: Mutex::new(HashMap::new()),
        }
    }

    /// Асинхронное чтение с внешним таймаутом.
    pub async fn read(&self, project_path: &Path) -> Result<StatusSnapshot, String> {
        let ctx = self.ctx(project_path);
        let what = project_path.display().to_string();
        let path = project_path.to_path_buf();
        let task = tauri::async_runtime::spawn_blocking(move || ctx.read(path));
        match tokio::time::timeout(STATUS_COMMAND_TIMEOUT, task).await {
            Ok(Ok(snapshot)) => Ok(snapshot),
            Ok(Err(err)) => Err(format!("Ошибка фоновой задачи: {err}")),
            Err(_) => {
                log::error!("get_project_status {what}: превышен внешний таймаут");
                Err("Таймаут получения статуса".to_string())
            }
        }
    }

    /// Синхронное чтение для потока, который уже не асинхронный
    /// (`spawn_blocking` команды записи либо поток watcher-а).
    pub fn read_blocking(&self, project_path: &Path) -> StatusSnapshot {
        self.ctx(project_path).read(project_path.to_path_buf())
    }

    fn ctx(&self, project_path: &Path) -> ReadCtx {
        ReadCtx {
            git: Arc::clone(&self.git),
            slots: Arc::clone(&self.slots),
            gate: self.gate(project_path),
            seq: Arc::clone(&self.seq),
        }
    }

    fn gate(&self, project_path: &Path) -> Arc<ReadGate> {
        let mut gates = self.gates.lock().unwrap_or_else(|e| e.into_inner());
        Arc::clone(
            gates
                .entry(project_path.to_path_buf())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }
}
