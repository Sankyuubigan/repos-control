use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{Event, RecursiveMode, RecommendedWatcher, Watcher};
use tauri::{AppHandle, Emitter};

use crate::domain::contracts::GitApi;
use crate::domain::project::ProjectStatus;
use crate::domain::usecases;

const DEBOUNCE: Duration = Duration::from_millis(1000);
const MIN_INTERVAL: Duration = Duration::from_secs(5);
const EVENT_NAME: &str = "status-changed";

type Sender = mpsc::Sender<PathBuf>;

#[derive(serde::Serialize)]
struct StatusPayload {
    path: String,
    #[serde(flatten)]
    status: ProjectStatus,
}

pub struct WatcherManager {
    watchers: Mutex<HashMap<PathBuf, RecommendedWatcher>>,
    sender: Sender,
    _worker: std::thread::JoinHandle<()>,
}

impl WatcherManager {
    pub fn new(
        app: AppHandle,
        git: Arc<dyn GitApi>,
        busy: Arc<AtomicBool>,
        slots: Arc<tokio::sync::Semaphore>,
    ) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel::<PathBuf>();
        let worker = std::thread::Builder::new()
            .name("fs-watcher".into())
            .spawn(move || worker_loop(&app, &receiver, git, busy, slots))
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
        "\\test\\last_logs.txt",
    ]
    .iter()
    .any(|seg| text.contains(seg))
}

fn worker_loop(
    app: &AppHandle,
    receiver: &mpsc::Receiver<PathBuf>,
    git: Arc<dyn GitApi>,
    busy: Arc<AtomicBool>,
    slots: Arc<tokio::sync::Semaphore>,
) {
    let mut pending: HashMap<PathBuf, Instant> = HashMap::new();
    let mut last_runs: HashMap<PathBuf, Instant> = HashMap::new();
    loop {
        fire_due(&app, &git, &busy, &slots, &mut pending, &mut last_runs);
        let wait = pending_deadline(&pending);
        let wait = match wait {
            Some(deadline) => deadline.saturating_duration_since(Instant::now()),
            None => Duration::MAX,
        };
        if wait.is_zero() {
            continue;
        }
        let wait = wait.min(Duration::from_secs(30));
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
    git: &Arc<dyn GitApi>,
    busy: &AtomicBool,
    slots: &Arc<tokio::sync::Semaphore>,
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
        if busy.load(Ordering::SeqCst) {
            log::info!("skip status refresh for {}: operation in progress", path.display());
            continue;
        }
        if last_runs
            .get(&path)
            .is_some_and(|prev| prev.elapsed() < MIN_INTERVAL)
        {
            log::info!("skip status refresh for {}: recent refresh", path.display());
            continue;
        }
        let app = app.clone();
        let git = Arc::clone(git);
        let slots = Arc::clone(slots);
        let path_in_thread = path.clone();
        if let Err(err) = std::thread::Builder::new()
            .name("status-refresh".into())
            .spawn(move || {
                let _permit = loop {
                    match slots.try_acquire() {
                        Ok(permit) => break permit,
                        Err(_) => std::thread::sleep(Duration::from_millis(100)),
                    }
                };
                run_and_emit(&app, git.as_ref(), &path_in_thread);
            })
        {
            log::warn!("status refresh thread spawn failed for {}: {err}", path.display());
        }
        last_runs.insert(path, Instant::now());
    }
}

fn run_and_emit(app: &AppHandle, git: &dyn GitApi, path: &Path) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        usecases::get_project_status(git, path)
    }));
    let status = match result {
        Ok(status) => status,
        Err(_) => {
            log::error!("status refresh panicked for {}", path.display());
            return;
        }
    };
    let payload = StatusPayload {
        path: path.to_string_lossy().into_owned(),
        status,
    };
    if let Err(err) = app.emit(EVENT_NAME, &payload) {
        log::warn!("status emit failed for {}: {err}", path.display());
    }
}