use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::domain::contracts::{DiffScope, GitApi};
use crate::domain::project::{ChangedFile, ProjectStatus};

mod ahead_behind;
mod diff;
mod snapshot;

const STATUS_TIMEOUT: Duration = Duration::from_secs(25);
const DIFF_TIMEOUT: Duration = Duration::from_secs(20);

/// Читатель состояния репозитория на библиотеке `gix`.
///
/// Единственный источник правды — рабочее дерево и `.git` на диске. Кэша нет
/// намеренно: закэшированный статус показывал состояние «до операции», из-за
/// чего stage/commit/push выглядели как ничего не сделавшие.
pub struct GixGit;

impl GixGit {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GixGit {
    fn default() -> Self {
        Self::new()
    }
}

/// Следит за превышением лимита времени: выставляет `interrupt`, который
/// проверяется внутри итераций `gix status` и обхода коммитов.
struct DeadlineGuard {
    done: Arc<AtomicBool>,
    interrupt: Arc<AtomicBool>,
    deadline: Instant,
}

impl DeadlineGuard {
    fn start(what: &'static str, timeout: Duration) -> Self {
        let done = Arc::new(AtomicBool::new(false));
        let interrupt = Arc::new(AtomicBool::new(false));
        let deadline = Instant::now() + timeout;
        spawn_watchdog(Arc::clone(&done), Arc::clone(&interrupt), deadline, what);
        Self {
            done,
            interrupt,
            deadline,
        }
    }

    fn interrupt(&self) -> &Arc<AtomicBool> {
        &self.interrupt
    }

    fn deadline(&self) -> Instant {
        self.deadline
    }

    fn finish(self) {
        self.done.store(true, Ordering::Release);
    }
}

fn spawn_watchdog(
    done: Arc<AtomicBool>,
    interrupt: Arc<AtomicBool>,
    deadline: Instant,
    what: &'static str,
) {
    if std::thread::Builder::new()
        .name("status-watchdog".into())
        .spawn(move || {
            while !done.load(Ordering::Acquire) {
                if Instant::now() >= deadline {
                    log::warn!("{what}: превышен лимит времени, прерываю");
                    interrupt.store(true, Ordering::Release);
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        })
        .is_err()
    {
        log::warn!("watchdog spawn failed for {what}");
    }
}

fn check_deadline(interrupt: &AtomicBool, deadline: Instant, stage: &str) -> Result<()> {
    if interrupt.load(Ordering::Acquire) || Instant::now() >= deadline {
        anyhow::bail!("{stage}: превышен лимит времени");
    }
    Ok(())
}

pub(super) fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn unstaged_with_untracked(snap: &snapshot::Snapshot) -> Vec<ChangedFile> {
    let mut files = snapshot::direct_changed_files(&snap.unstaged, 'M');
    files.extend(snapshot::direct_changed_files(&snap.untracked, 'U'));
    files
}

impl GitApi for GixGit {
    fn status(&self, project_path: &Path) -> Result<ProjectStatus> {
        let started = Instant::now();
        let guard = DeadlineGuard::start("status", STATUS_TIMEOUT);
        let result = (|| {
            let repo = snapshot::open_repo(project_path)?;
            let snap = snapshot::collect_snapshot(&repo, guard.interrupt(), guard.deadline())?;
            let (ahead, behind, has_upstream) = match (snap.head_id, snap.upstream_id) {
                (Some(head), Some(upstream)) => {
                    let (a, b) = ahead_behind::count(
                        &repo,
                        head,
                        upstream,
                        guard.interrupt(),
                        guard.deadline(),
                    )?;
                    (a, b, true)
                }
                _ => (0, 0, false),
            };
            let changed = snapshot::changed_files(&snap);
            let staged_files = snapshot::direct_changed_files(&snap.staged, 'A');
            let unstaged_files = unstaged_with_untracked(&snap);
            let (staged, unstaged, untracked) = (snap.staged.len(), snap.unstaged.len(), snap.untracked.len());
            let branch = snap.branch;
            Ok(ProjectStatus {
                is_repo: true,
                branch,
                has_upstream,
                ahead,
                behind,
                staged,
                unstaged,
                untracked,
                changed_files: changed,
                staged_files,
                unstaged_files,
                error: None,
            })
        })();
        guard.finish();
        match &result {
            Ok(_) => log::info!(
                "статус {}: {:.1}с",
                project_path.display(),
                started.elapsed().as_secs_f64()
            ),
            Err(err) => log::error!("статус {} не удался: {err:#}", project_path.display()),
        }
        result
    }

    fn collect_diff(&self, project_path: &Path, scope: DiffScope) -> Result<String> {
        let started = Instant::now();
        let guard = DeadlineGuard::start("diff", DIFF_TIMEOUT);
        let result = (|| {
            let repo = snapshot::open_repo(project_path)?;
            let snap = snapshot::collect_snapshot(&repo, guard.interrupt(), guard.deadline())?;
            diff::collect_diff(&repo, &snap, scope, guard.interrupt(), guard.deadline())
        })();
        guard.finish();
        match &result {
            Ok(_) => log::info!(
                "diff {}: {:.1}с",
                project_path.display(),
                started.elapsed().as_secs_f64()
            ),
            Err(err) => log::error!("diff {} не удался: {err:#}", project_path.display()),
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::domain::contracts::{DiffScope, GitWriteApi};
    use crate::infra::git2_write::Git2Write;

    use super::*;

    fn temp_workdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "repos-control-status-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git2::Repository::init(&dir).unwrap();
        dir
    }

    #[test]
    fn lossy_converts() {
        assert_eq!(lossy(b"hello"), "hello");
    }

    /// Регрессия: раньше `status()` отдавал результат из TTL-кэша (10 секунд),
    /// поэтому после stage/commit UI показывал состояние «до операции» и
    /// выглядел так, будто запись ничего не сделала.
    #[test]
    fn status_reflects_filesystem_immediately() {
        let workdir = temp_workdir("no-stale-status");
        let gix = GixGit::new();

        let before = gix.status(&workdir).expect("status до записи");
        assert_eq!(before.untracked, 0);

        std::fs::write(workdir.join("new.txt"), "x").expect("создать файл");
        let after_write = gix.status(&workdir).expect("status после создания файла");
        assert_eq!(after_write.untracked, 1, "новый файл обязан быть виден сразу");

        Git2Write
            .stage(&workdir, &["new.txt".to_string()])
            .expect("stage");
        let after_stage = gix.status(&workdir).expect("status после stage");
        assert_eq!(after_stage.staged, 1, "запись обязана быть видна сразу");
        assert_eq!(after_stage.untracked, 0);

        std::fs::write(workdir.join("new.txt"), "changed").expect("изменить файл");
        let after_edit = gix.status(&workdir).expect("status после правки");
        assert_eq!(after_edit.unstaged, 1, "изменение файла обязано быть видно сразу");
        assert_eq!(after_edit.staged, 1);
    }

    #[test]
    fn status_of_unborn_repo_reports_no_upstream() {
        let workdir = temp_workdir("unborn");
        let status = GixGit::new().status(&workdir).expect("status");
        assert!(status.is_repo);
        assert!(!status.has_upstream);
        assert_eq!(status.ahead, 0);
        assert_eq!(status.behind, 0);
        assert_eq!(status.staged, 0);
        assert_eq!(status.unstaged, 0);
        assert_eq!(status.untracked, 0);
        assert!(status.error.is_none());
    }

    #[test]
    fn diff_of_empty_scope_is_empty_string() {
        let workdir = temp_workdir("empty-diff");
        let diff = GixGit::new()
            .collect_diff(&workdir, DiffScope::All)
            .expect("diff");
        assert!(diff.trim().is_empty(), "без изменений diff обязан быть пустым");
    }
}
