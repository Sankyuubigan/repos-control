use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{Event, RecursiveMode, RecommendedWatcher, Watcher};
use tauri::{AppHandle, Emitter};

use crate::infra::status_hub::{StatusHub, StatusSnapshot};
use crate::infra::write_registry::WriteRegistry;

/// Пауза перед перечитыванием после события файловой системы: жмём один раз
/// после серии правок, а не после каждого файла.
const DEBOUNCE: Duration = Duration::from_millis(1000);
/// Минимальный интервал между перечитываниями одного проекта.
const MIN_INTERVAL: Duration = Duration::from_secs(5);
/// Пауза перед повторной попыткой, если перечитывание не удалось выполнить
/// сейчас (идёт запись или сработал троттлинг).
const RETRY_DELAY: Duration = Duration::from_millis(1000);
const EVENT_NAME: &str = "status-changed";
const IDLE_TICK: Duration = Duration::from_secs(30);

type Sender = mpsc::Sender<PathBuf>;

pub struct WatcherManager {
    watchers: Mutex<HashMap<PathBuf, RecommendedWatcher>>,
    sender: Sender,
    _worker: std::thread::JoinHandle<()>,
}

impl WatcherManager {
    pub fn new(
        app: AppHandle,
        status: Arc<StatusHub>,
        writes: Arc<WriteRegistry>,
    ) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel::<PathBuf>();
        let worker = std::thread::Builder::new()
            .name("fs-watcher".into())
            .spawn(move || worker_loop(&app, &receiver, status, writes))
            .map_err(|err| format!("cannot spawn fs-watcher thread: {err}"))?;
        Ok(Self {
            watchers: Mutex::new(HashMap::new()),
            sender,
            _worker: worker,
        })
    }

    pub fn set_project_paths(&self, paths: &[PathBuf]) {
        let mut watchers = self.watchers.lock().unwrap_or_else(|e| e.into_inner());
        watchers.retain(|key, _| paths.iter().any(|p| p == key));
        for path in paths {
            if watchers.contains_key(path) {
                continue;
            }
            if let Some(watcher) = make_watcher(path, self.sender.clone()) {
                watchers.insert(path.clone(), watcher);
            }
        }
    }
}

fn make_watcher(path: &Path, sender: Sender) -> Option<RecommendedWatcher> {
    let project = path.to_path_buf();
    let event_sender = sender.clone();
    let mut watcher = match notify::recommended_watcher(move |res: notify::Result<Event>| {
        match res {
            Ok(event) => {
                if is_interesting(&event) && event_sender.send(project.clone()).is_err() {
                    log::warn!("watcher channel closed for {}", project.display());
                }
            }
            Err(err) => log::warn!("watcher event error for {}: {err}", project.display()),
        }
    }) {
        Ok(w) => w,
        Err(err) => {
            log::warn!("cannot create watcher for {}: {err}", path.display());
            return None;
        }
    };
    if let Err(err) = watcher.watch(path, RecursiveMode::Recursive) {
        log::warn!("cannot watch {}: {err}", path.display());
        return None;
    }
    log::info!("watching {}", path.display());
    Some(watcher)
}

fn is_interesting(event: &Event) -> bool {
    event.paths.iter().any(|p| !is_noise(p))
}

fn is_noise(path: &Path) -> bool {
    let text = path.to_string_lossy().replace('/', "\\").to_ascii_lowercase();
    [
        "\\node_modules",
        "\\target",
        "\\.git\\objects",
        "\\.git\\tmp",
        // Служебная блокировка индекса: меняется на каждый чих, статуса не касается.
        "\\.git\\index.lock",
        "\\test\\last_logs.txt",
    ]
    .iter()
    .any(|seg| text.contains(seg))
}

fn worker_loop(
    app: &AppHandle,
    receiver: &mpsc::Receiver<PathBuf>,
    status: Arc<StatusHub>,
    writes: Arc<WriteRegistry>,
) {
    let mut pending: HashMap<PathBuf, Instant> = HashMap::new();
    let mut last_runs: HashMap<PathBuf, Instant> = HashMap::new();
    loop {
        fire_due(
            &app,
            &status,
            &writes,
            &mut pending,
            &mut last_runs,
        );
        let wait = match pending_deadline(&pending) {
            Some(deadline) => deadline.saturating_duration_since(Instant::now()),
            None => IDLE_TICK,
        };
        let wait = wait.min(IDLE_TICK);
        if wait.is_zero() {
            continue;
        }
        match receiver.recv_timeout(wait) {
            Ok(path) => {
                pending.insert(path, Instant::now() + DEBOUNCE);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn pending_deadline(pending: &HashMap<PathBuf, Instant>) -> Option<Instant> {
    pending.values().min().copied()
}

fn fire_due(
    app: &AppHandle,
    status: &Arc<StatusHub>,
    writes: &Arc<WriteRegistry>,
    pending: &mut HashMap<PathBuf, Instant>,
    last_runs: &mut HashMap<PathBuf, Instant>,
) {
    let now = Instant::now();
    let due: Vec<PathBuf> = pending
        .iter()
        .filter(|(_, deadline)| **deadline <= now)
        .map(|(path, _)| path.clone())
        .collect();
    for path in due {
        pending.remove(&path);
        // Событие НИКОГДА не теряется: если перечитать сейчас нельзя, оно
        // перевзводится в очередь и будет обработано позже. Раньше здесь был
        // `continue` без перевзвода — изменение пропадало навсегда.
        if writes.is_writing(&path) {
            log::debug!("status {} отложен: идёт запись", path.display());
            defer(pending, &path);
            continue;
        }
        if last_runs
            .get(&path)
            .is_some_and(|prev| prev.elapsed() < MIN_INTERVAL)
        {
            log::debug!("status {} отложен: недавнее перечитывание", path.display());
            defer(pending, &path);
            continue;
        }
        let app = app.clone();
        let status = Arc::clone(status);
        let path_in_thread = path.clone();
        if let Err(err) = std::thread::Builder::new()
            .name("status-refresh".into())
            .spawn(move || {
                run_and_emit(&app, status.as_ref(), &path_in_thread);
            })
        {
            log::warn!("status refresh thread spawn failed for {}: {err}", path.display());
            defer(pending, &path);
            continue;
        }
        last_runs.insert(path, Instant::now());
    }
}

fn defer(pending: &mut HashMap<PathBuf, Instant>, path: &Path) {
    pending.insert(path.to_path_buf(), Instant::now() + RETRY_DELAY);
}

fn run_and_emit(app: &AppHandle, status: &StatusHub, path: &Path) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        status.read_blocking(path)
    }));
    let snapshot: StatusSnapshot = match result {
        Ok(snapshot) => snapshot,
        Err(_) => {
            log::error!("status refresh panicked for {}", path.display());
            return;
        }
    };
    if let Err(err) = app.emit(EVENT_NAME, &snapshot) {
        log::warn!("status emit failed for {}: {err}", path.display());
    }
}
