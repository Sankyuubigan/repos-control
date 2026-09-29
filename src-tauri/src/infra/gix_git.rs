use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use gix::bstr::{BString, ByteSlice};
use gix::diff::blob::unified_diff::{ConsumeBinaryHunk, ContextSize};
use gix::diff::blob::{diff_with_slider_heuristics, Algorithm, InternedInput, UnifiedDiff};

use crate::domain::contracts::{DiffScope, GitApi};
use crate::domain::project::{ChangeKind, ChangedFile, FileChange, ProjectStatus};

pub struct GixGit {
    cache: Mutex<HashMap<PathBuf, CachedStatus>>,
}

struct CachedStatus {
    status: ProjectStatus,
    at: Instant,
}

impl GixGit {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn cached(&self, project_path: &Path) -> Option<ProjectStatus> {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        match cache.get(project_path) {
            Some(entry) if entry.at.elapsed() < STATUS_CACHE_TTL => Some(entry.status.clone()),
            _ => {
                cache.remove(project_path);
                None
            }
        }
    }

    fn remember(&self, project_path: &Path, status: &ProjectStatus) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.insert(
            project_path.to_path_buf(),
            CachedStatus {
                status: status.clone(),
                at: Instant::now(),
            },
        );
    }
}

impl Default for GixGit {
    fn default() -> Self {
        Self::new()
    }
}

struct Snapshot {
    branch: Option<String>,
    head_id: Option<gix::hash::ObjectId>,
    upstream_id: Option<gix::hash::ObjectId>,
    staged: Vec<FileChange>,
    unstaged: Vec<FileChange>,
    untracked: Vec<FileChange>,
}

const DIFF_LINE_LIMIT: usize = 500;
const DIFF_CONTEXT: u32 = 3;
const BINARY_SNOOP: usize = 8000;
const STATUS_CACHE_TTL: Duration = Duration::from_secs(10);
const STATUS_TIMEOUT: Duration = Duration::from_secs(25);
const DIFF_TIMEOUT: Duration = Duration::from_secs(20);

fn spawn_watchdog(done: Arc<AtomicBool>, interrupt: Arc<AtomicBool>, deadline: Instant, what: &'static str) {
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

impl GitApi for GixGit {
    fn status(&self, project_path: &Path) -> Result<ProjectStatus> {
        if let Some(status) = self.cached(project_path) {
            return Ok(status);
        }
        let started = Instant::now();
        let done = Arc::new(AtomicBool::new(false));
        let interrupt = Arc::new(AtomicBool::new(false));
        let deadline = Instant::now() + STATUS_TIMEOUT;
        spawn_watchdog(Arc::clone(&done), Arc::clone(&interrupt), deadline, "status");
        let result = (|| {
            let repo = open_repo(project_path)?;
            let snap = collect_snapshot(&repo, &interrupt, deadline)?;
            let (ahead, behind, has_upstream) = match (snap.head_id, snap.upstream_id) {
                (Some(head), Some(upstream)) => {
                    let (a, b) = count_ahead_behind(&repo, head, upstream, &interrupt, deadline)?;
                    (a, b, true)
                }
                _ => (0, 0, false),
            };
            let changed_files = collect_changed_files(&snap);
            let staged_files = direct_changed_files(&snap.staged, 'A');
            let mut unstaged_files = direct_changed_files(&snap.unstaged, 'M');
            unstaged_files.extend(direct_changed_files(&snap.untracked, 'U'));
            Ok(ProjectStatus {
                is_repo: true,
                branch: snap.branch,
                has_upstream,
                ahead,
                behind,
                staged: snap.staged.len(),
                unstaged: snap.unstaged.len(),
                untracked: snap.untracked.len(),
                changed_files,
                staged_files,
                unstaged_files,
                error: None,
            })
        })();
        done.store(true, Ordering::Release);
        if let Ok(status) = &result {
            log::info!(
                "статус {}: {:.1}с",
                project_path.display(),
                started.elapsed().as_secs_f64()
            );
            self.remember(project_path, status);
        } else if let Err(err) = &result {
            log::error!("статус {} не удался: {err:#}", project_path.display());
        }
        result
    }

    fn collect_diff(&self, project_path: &Path, scope: DiffScope) -> Result<String> {
        let started = Instant::now();
        let done = Arc::new(AtomicBool::new(false));
        let interrupt = Arc::new(AtomicBool::new(false));
        let deadline = Instant::now() + DIFF_TIMEOUT;
        spawn_watchdog(
            Arc::clone(&done),
            Arc::clone(&interrupt),
            deadline,
            "diff",
        );
        let result = (|| {
            let repo = open_repo(project_path)?;
            let snap = collect_snapshot(&repo, &interrupt, deadline)?;

            let entries: Vec<&FileChange> = match scope {
                DiffScope::Staged => snap
                    .staged
                    .iter()
                    .filter(|c| c.kind != ChangeKind::Removed)
                    .collect(),
                DiffScope::Unstaged => {
                    let mut v: Vec<&FileChange> = snap
                        .unstaged
                        .iter()
                        .filter(|c| c.kind != ChangeKind::Removed)
                        .collect();
                    v.extend(snap.untracked.iter());
                    v
                }
                DiffScope::All => {
                    let mut v: Vec<&FileChange> = snap
                        .staged
                        .iter()
                        .filter(|c| c.kind != ChangeKind::Removed)
                        .collect();
                    v.extend(
                        snap.unstaged
                            .iter()
                            .filter(|c| c.kind != ChangeKind::Removed),
                    );
                    v.extend(snap.untracked.iter());
                    v
                }
            };

            let mut blocks: Vec<String> = Vec::new();
            for entry in entries {
                check_deadline(&interrupt, deadline, "diff")?;
                if let Some(block) = render_file_diff(&repo, entry)? {
                    blocks.push(block);
                }
            }
            if blocks.is_empty() {
                return Ok(String::new());
            }
            let joined = blocks.join("\n");
            Ok(truncate_lines(&joined, DIFF_LINE_LIMIT))
        })();
        done.store(true, Ordering::Release);
        if let Err(err) = &result {
            log::error!(
                "diff {} не удался: {err:#}",
                project_path.display()
            );
        } else {
            log::info!(
                "diff {}: {:.1}с",
                project_path.display(),
                started.elapsed().as_secs_f64()
            );
        }
        result
    }
}

fn open_repo(path: &Path) -> Result<gix::Repository> {
    gix::discover(path).with_context(|| format!("Не git-репозиторий: {}", path.display()))
}

fn collect_snapshot(
    repo: &gix::Repository,
    interrupt: &Arc<AtomicBool>,
    deadline: Instant,
) -> Result<Snapshot> {
    let workdir = repo
        .workdir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let t0 = Instant::now();
    let head = repo.head().with_context(|| "read HEAD")?;
    let (branch, head_id) = resolve_head(repo, &head)?;
    let t_head = Instant::now();

    let upstream_id = match &branch {
        Some(name) => find_upstream(repo, name)?,
        None => None,
    };

    let t_status = Instant::now();
    let mut staged: Vec<FileChange> = Vec::new();
    let mut unstaged: Vec<FileChange> = Vec::new();
    let mut untracked: Vec<FileChange> = Vec::new();

    let iter = repo
        .status(gix::progress::Discard)?
        .should_interrupt_owned(interrupt.clone())
        .untracked_files(gix::status::UntrackedFiles::Files)
        .into_iter(Vec::<BString>::new())
        .context("run git status")?;

    for item in iter {
        check_deadline(interrupt, deadline, "status")?;
        let item = item.context("status iteration")?;
        match item {
            gix::status::Item::TreeIndex(change) => push_tree_index(&change, &mut staged),
            gix::status::Item::IndexWorktree(inner) => {
                handle_index_worktree_item(repo, inner, &mut unstaged, &mut untracked)?;
            }
        }
    }
    check_deadline(interrupt, deadline, "status")?;

    staged.sort_by(|a, b| a.path.cmp(&b.path));
    unstaged.sort_by(|a, b| a.path.cmp(&b.path));
    untracked.sort_by(|a, b| a.path.cmp(&b.path));

    log::debug!(
        "snapshot {workdir}: head {:.0}мс, upstream {:.0}мс, status-iter {:.0}мс",
        t_head.duration_since(t0).as_millis(),
        t_status.duration_since(t_head).as_millis(),
        t_status.elapsed().as_millis()
    );

    Ok(Snapshot {
        branch,
        head_id,
        upstream_id,
        staged,
        unstaged,
        untracked,
    })
}

fn resolve_head(
    repo: &gix::Repository,
    head: &gix::Head,
) -> Result<(Option<String>, Option<gix::hash::ObjectId>)> {
    if head.is_unborn() {
        return Ok((Some("(нет коммитов)".to_string()), None));
    }
    let head_id = Some(repo.head_id().context("resolve HEAD id")?.detach());
    match head.referent_name() {
        Some(name) => {
            let full = lossy(name.as_bstr().as_ref());
            let short = full
                .strip_prefix("refs/heads/")
                .map(|s| s.to_string())
                .unwrap_or_else(|| full.clone());
            Ok((Some(short), head_id))
        }
        None => Ok((Some("(отсоединённый HEAD)".to_string()), head_id)),
    }
}

fn find_upstream(repo: &gix::Repository, branch: &str) -> Result<Option<gix::hash::ObjectId>> {
    let branch_bytes = branch.as_bytes();
    let mut found: Option<gix::hash::ObjectId> = None;
    let refs = repo.references().context("read references")?;
    for entry in refs.all()? {
        let Ok(reference) = entry else {
            continue;
        };
        let name = reference.name().as_bstr();
        let bytes: &[u8] = name.as_ref();
        let Some(rest) = bytes.strip_prefix(b"refs/remotes/") else {
            continue;
        };
        let Some(sep) = rest.iter().position(|&b| b == b'/') else {
            continue;
        };
        if &rest[sep + 1..] != branch_bytes {
            continue;
        }
        let oid = reference.id().detach();
        if &rest[..sep] == b"origin" {
            return Ok(Some(oid));
        }
        if found.is_none() {
            found = Some(oid);
        }
    }
    Ok(found)
}

/// Плоский список изменённых файлов для UI-тугла «Файлы (N)»: отсортирован по
/// пути, один файл = одна строка. Приоритет статуса: staged > unstaged > untracked.
fn collect_changed_files(snap: &Snapshot) -> Vec<ChangedFile> {
    let mut by_path: BTreeMap<String, char> = BTreeMap::new();
    for change in &snap.staged {
        by_path.insert(change.path.clone(), change_kind_char(change.kind));
    }
    for change in &snap.unstaged {
        by_path
            .entry(change.path.clone())
            .or_insert_with(|| change_kind_char(change.kind));
    }
    for change in &snap.untracked {
        by_path.entry(change.path.clone()).or_insert('A');
    }
    by_path
        .into_iter()
        .map(|(path, status)| ChangedFile {
            path,
            status: status.to_string(),
        })
        .collect()
}

fn change_kind_char(kind: ChangeKind) -> char {
    match kind {
        ChangeKind::Added => 'A',
        ChangeKind::Modified => 'M',
        ChangeKind::Removed => 'D',
    }
}

fn direct_changed_files(changes: &[FileChange], untracked_status: char) -> Vec<ChangedFile> {
    changes
        .iter()
        .map(|change| {
            let status = if change.kind == ChangeKind::Added
                && change.old_oid.is_none()
                && change.new_oid.is_none()
            {
                untracked_status
            } else {
                change_kind_char(change.kind)
            };
            ChangedFile {
                path: change.path.clone(),
                status: status.to_string(),
            }
        })
        .collect()
}

fn push_tree_index(change: &gix::diff::index::ChangeRef, staged: &mut Vec<FileChange>) {
    use gix::diff::index::ChangeRef;

    let (path, old_oid, new_oid, kind) = match change {
        ChangeRef::Addition { location, id, .. } => {
            (lossy(path_ref(location).as_ref()), None, Some(cow_oid(id)), ChangeKind::Added)
        }
        ChangeRef::Deletion { location, id, .. } => {
            (lossy(path_ref(location).as_ref()), Some(cow_oid(id)), None, ChangeKind::Removed)
        }
        ChangeRef::Modification { previous_id, location, id, .. } => (
            lossy(path_ref(location).as_ref()),
            Some(cow_oid(previous_id)),
            Some(cow_oid(id)),
            ChangeKind::Modified,
        ),
        ChangeRef::Rewrite { source_id, location, id, .. } => (
            lossy(path_ref(location).as_ref()),
            Some(cow_oid(source_id)),
            Some(cow_oid(id)),
            ChangeKind::Modified,
        ),
    };
    staged.push(FileChange {
        path,
        kind,
        old_oid,
        new_oid,
        worktree: None,
    });
}

fn path_ref<'a>(cow: &'a std::borrow::Cow<'_, gix::bstr::BStr>) -> &'a gix::bstr::BStr {
    cow.as_ref()
}

fn cow_oid(cow: &std::borrow::Cow<'_, gix::hash::oid>) -> gix::hash::ObjectId {
    cow.as_ref().to_owned()
}

fn handle_index_worktree_item(
    repo: &gix::Repository,
    item: gix::status::index_worktree::Item,
    unstaged: &mut Vec<FileChange>,
    untracked: &mut Vec<FileChange>,
) -> Result<()> {
    use gix::status::plumbing::index_as_worktree::{Change, EntryStatus};

    match item {
        gix::status::index_worktree::Item::Modification {
            entry, rela_path, status, ..
        } => {
            let rel = rela_path.as_bstr();
            let path = lossy(rel.as_ref());
            let worktree = repo.workdir_path(rel);
            match status {
                EntryStatus::Change(Change::Removed) => {
                    unstaged.push(FileChange {
                        path,
                        kind: ChangeKind::Removed,
                        old_oid: Some(entry.id),
                        new_oid: None,
                        worktree,
                    });
                }
                EntryStatus::Change(_) | EntryStatus::Conflict { .. } => {
                    unstaged.push(FileChange {
                        path,
                        kind: ChangeKind::Modified,
                        old_oid: Some(entry.id),
                        new_oid: None,
                        worktree,
                    });
                }
                _ => {}
            }
        }
        gix::status::index_worktree::Item::DirectoryContents { entry, .. } => {
            if entry.status == gix::dir::entry::Status::Untracked
                && !entry.disk_kind.is_some_and(|k| k.is_dir())
            {
                let rel = entry.rela_path.as_bstr();
                let worktree = repo.workdir_path(rel);
                untracked.push(FileChange {
                    path: lossy(rel.as_ref()),
                    kind: ChangeKind::Added,
                    old_oid: None,
                    new_oid: None,
                    worktree,
                });
            }
        }
        _ => {}
    }
    Ok(())
}

fn count_ahead_behind(
    repo: &gix::Repository,
    head: gix::hash::ObjectId,
    upstream: gix::hash::ObjectId,
    interrupt: &AtomicBool,
    deadline: Instant,
) -> Result<(u32, u32)> {
    let upstream_set = reachable(repo, upstream, interrupt, deadline)?;
    let head_set = reachable(repo, head, interrupt, deadline)?;
    let ahead = walk_unique(repo, head, &upstream_set, interrupt, deadline)?;
    let behind = walk_unique(repo, upstream, &head_set, interrupt, deadline)?;
    Ok((ahead, behind))
}

fn reachable(
    repo: &gix::Repository,
    tip: gix::hash::ObjectId,
    interrupt: &AtomicBool,
    deadline: Instant,
) -> Result<HashSet<gix::hash::ObjectId>> {
    let mut seen: HashSet<gix::hash::ObjectId> = HashSet::new();
    let mut stack = vec![tip];
    while let Some(id) = stack.pop() {
        check_deadline(interrupt, deadline, "ahead_behind")?;
        if !seen.insert(id) {
            continue;
        }
        let commit = repo.find_commit(id).with_context(|| format!("find commit {id}"))?;
        stack.extend(commit.parent_ids().map(|p| p.detach()));
    }
    Ok(seen)
}

fn walk_unique(
    repo: &gix::Repository,
    tip: gix::hash::ObjectId,
    shared: &HashSet<gix::hash::ObjectId>,
    interrupt: &AtomicBool,
    deadline: Instant,
) -> Result<u32> {
    let mut seen: HashSet<gix::hash::ObjectId> = HashSet::new();
    let mut count: u32 = 0;
    let mut stack = vec![tip];
    while let Some(id) = stack.pop() {
        check_deadline(interrupt, deadline, "ahead_behind")?;
        if shared.contains(&id) {
            continue;
        }
        if !seen.insert(id) {
            continue;
        }
        count += 1;
        let commit = repo.find_commit(id).with_context(|| format!("find commit {id}"))?;
        stack.extend(commit.parent_ids().map(|p| p.detach()));
    }
    Ok(count)
}

fn render_file_diff(repo: &gix::Repository, entry: &FileChange) -> Result<Option<String>> {
    let before = read_before(repo, entry);
    let after = read_after(repo, entry)?;

    let p = &entry.path;
    let header = match entry.kind {
        ChangeKind::Added => {
            format!("diff --git a/{p} b/{p}\nnew file mode 100644\n--- /dev/null\n+++ b/{p}")
        }
        ChangeKind::Removed => {
            format!("diff --git a/{p} b/{p}\ndeleted file mode 100644\n--- a/{p}\n+++ /dev/null")
        }
        ChangeKind::Modified => format!("diff --git a/{p} b/{p}\n--- a/{p}\n+++ b/{p}"),
    };

    if looks_binary(&before) || looks_binary(&after) {
        return Ok(Some(format!("{header}\nBinary files differ\n")));
    }

    let hunks = render_hunks(&before, &after)?;
    if hunks.trim().is_empty() {
        return Ok(Some(format!("{header}\n")));
    }
    Ok(Some(format!("{header}\n{hunks}")))
}

fn read_before(repo: &gix::Repository, entry: &FileChange) -> Vec<u8> {
    match entry.old_oid {
        Some(oid) => read_blob(repo, oid),
        None => Vec::new(),
    }
}

fn read_after(repo: &gix::Repository, entry: &FileChange) -> Result<Vec<u8>> {
    if let Some(oid) = entry.new_oid {
        return Ok(read_blob(repo, oid));
    }
    match &entry.worktree {
        Some(path) => {
            if path.is_dir() {
                return Ok(Vec::new());
            }
            std::fs::read(path).with_context(|| format!("read worktree file {}", path.display()))
        }
        None => Ok(Vec::new()),
    }
}

fn read_blob(repo: &gix::Repository, oid: gix::hash::ObjectId) -> Vec<u8> {
    match repo.find_blob(oid) {
        Ok(mut blob) => blob.take_data(),
        Err(err) => {
            log::warn!("read blob {oid} failed: {err:#}");
            Vec::new()
        }
    }
}

fn render_hunks(before: &[u8], after: &[u8]) -> Result<String> {
    let input = InternedInput::new(before, after);
    let diff = diff_with_slider_heuristics(Algorithm::Histogram, &input);
    let out = UnifiedDiff::new(
        &diff,
        &input,
        ConsumeBinaryHunk::new(String::new(), "\n"),
        ContextSize::symmetrical(DIFF_CONTEXT),
    )
    .consume()?;
    Ok(out)
}

fn looks_binary(bytes: &[u8]) -> bool {
    let limit = bytes.len().min(BINARY_SNOOP);
    bytes[..limit].contains(&0)
}

fn truncate_lines(text: &str, limit: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= limit {
        return text.to_string();
    }
    let head = (limit * 20 / 100).max(1);
    let tail = limit - head;
    let omitted = lines.len() - limit;
    let mut out = String::new();
    for line in &lines[..head] {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&format!("[...{omitted} lines omitted...]\n"));
    for line in &lines[lines.len() - tail..] {
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_keeps_head_and_tail() {
        let text = (0..10).map(|i| format!("line{i}")).collect::<Vec<_>>().join("\n");
        let out = truncate_lines(&text, 4);
        assert!(out.contains("[...6 lines omitted...]"));
        assert!(out.starts_with("line0\n"));
        assert!(out.ends_with("line9\n") || out.ends_with("line9"));
    }

    #[test]
    fn truncate_short_untouched() {
        let text = "a\nb\nc";
        assert_eq!(truncate_lines(text, 10), text);
    }

    #[test]
    fn binary_detected_on_null_byte() {
        assert!(looks_binary(&[1, 0, 2]));
        assert!(!looks_binary(b"plain text"));
    }

    #[test]
    fn lossy_converts() {
        assert_eq!(lossy(b"hello"), "hello");
    }

    fn temp_repo(name: &str) -> git2::Repository {
        let dir = std::env::temp_dir().join(format!(
            "repos-control-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git2::Repository::init(&dir).unwrap()
    }

    #[test]
    fn snapshot_lists_untracked_files_in_new_dir() {
        let repo = temp_repo("untracked-dir");
        let workdir = repo.workdir().unwrap().to_path_buf();
        let new_dir = workdir.join("new_dir");
        std::fs::create_dir_all(&new_dir).unwrap();
        std::fs::write(new_dir.join("a.txt"), "a").unwrap();
        std::fs::write(new_dir.join("b.txt"), "b").unwrap();

        let gix_repo = open_repo(&workdir).unwrap();
        let interrupt = Arc::new(AtomicBool::new(false));
        let snap = collect_snapshot(&gix_repo, &interrupt, Instant::now() + STATUS_TIMEOUT).unwrap();
        assert_eq!(snap.untracked.len(), 2, "файлы из новой папки не свернуты в директорию");
    }

    fn change(path: &str, kind: ChangeKind) -> FileChange {
        FileChange {
            path: path.to_string(),
            kind,
            old_oid: None,
            new_oid: None,
            worktree: None,
        }
    }

    #[test]
    fn changed_files_sorted_deduped_with_staged_priority() {
        let snap = Snapshot {
            branch: None,
            head_id: None,
            upstream_id: None,
            staged: vec![
                change("z.txt", ChangeKind::Modified),
                change("a.txt", ChangeKind::Added),
            ],
            unstaged: vec![
                change("a.txt", ChangeKind::Modified),
                change("m.txt", ChangeKind::Modified),
            ],
            untracked: vec![
                change("m.txt", ChangeKind::Added),
                change("u.txt", ChangeKind::Added),
            ],
        };
        let files = collect_changed_files(&snap);
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["a.txt", "m.txt", "u.txt", "z.txt"]);
        let status_of = |p: &str| files.iter().find(|f| f.path == p).unwrap().status.as_str();
        assert_eq!(status_of("a.txt"), "A", "staged Added имеет приоритет над unstaged Modified");
        assert_eq!(status_of("m.txt"), "M", "unstaged Modified не перезаписывается untracked");
        assert_eq!(status_of("u.txt"), "A");
        assert_eq!(status_of("z.txt"), "M");
    }

    #[test]
    fn changed_files_maps_removed_status() {
        let snap = Snapshot {
            branch: None,
            head_id: None,
            upstream_id: None,
            staged: vec![change("gone.txt", ChangeKind::Removed)],
            unstaged: vec![],
            untracked: vec![],
        };
        let files = collect_changed_files(&snap);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "gone.txt");
        assert_eq!(files[0].status, "D");
    }
}

#[allow(dead_code)]
type _Unused = PathBuf;