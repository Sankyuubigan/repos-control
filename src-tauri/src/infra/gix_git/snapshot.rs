use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use anyhow::{Context, Result};
use gix::bstr::{BString, ByteSlice};

use crate::domain::project::{ChangeKind, ChangedFile, FileChange};

use super::lossy;

pub(super) struct Snapshot {
    pub branch: Option<String>,
    pub head_id: Option<gix::hash::ObjectId>,
    pub upstream_id: Option<gix::hash::ObjectId>,
    pub staged: Vec<FileChange>,
    pub unstaged: Vec<FileChange>,
    pub untracked: Vec<FileChange>,
}

pub(super) fn open_repo(path: &Path) -> Result<gix::Repository> {
    gix::discover(path).with_context(|| format!("Не git-репозиторий: {}", path.display()))
}

pub(super) fn collect_snapshot(
    repo: &gix::Repository,
    interrupt: &std::sync::Arc<AtomicBool>,
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
        super::check_deadline(interrupt, deadline, "status")?;
        let item = item.context("status iteration")?;
        match item {
            gix::status::Item::TreeIndex(change) => push_tree_index(&change, &mut staged),
            gix::status::Item::IndexWorktree(inner) => {
                handle_index_worktree_item(repo, inner, &mut unstaged, &mut untracked)?;
            }
        }
    }
    super::check_deadline(interrupt, deadline, "status")?;

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
pub(super) fn changed_files(snap: &Snapshot) -> Vec<ChangedFile> {
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

pub(super) fn direct_changed_files(
    changes: &[FileChange],
    untracked_status: char,
) -> Vec<ChangedFile> {
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
        ChangeRef::Addition { location, id, .. } => (
            lossy(path_ref(location).as_ref()),
            None,
            Some(cow_oid(id)),
            ChangeKind::Added,
        ),
        ChangeRef::Deletion { location, id, .. } => (
            lossy(path_ref(location).as_ref()),
            Some(cow_oid(id)),
            None,
            ChangeKind::Removed,
        ),
        ChangeRef::Modification {
            previous_id,
            location,
            id,
            ..
        } => (
            lossy(path_ref(location).as_ref()),
            Some(cow_oid(previous_id)),
            Some(cow_oid(id)),
            ChangeKind::Modified,
        ),
        ChangeRef::Rewrite {
            source_id,
            location,
            id,
            ..
        } => (
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;

    const STATUS_TIMEOUT: Duration = Duration::from_secs(25);

    fn temp_repo(name: &str) -> git2::Repository {
        let dir = std::env::temp_dir().join(format!(
            "repos-control-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git2::Repository::init(&dir).unwrap()
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
        let files = changed_files(&snap);
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
        let files = changed_files(&snap);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, "D");
    }
}
