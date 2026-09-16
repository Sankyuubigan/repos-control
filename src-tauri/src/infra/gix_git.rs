use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use gix::bstr::{BString, ByteSlice};
use gix::diff::blob::unified_diff::{ConsumeBinaryHunk, ContextSize};
use gix::diff::blob::{diff_with_slider_heuristics, Algorithm, InternedInput, UnifiedDiff};

use crate::domain::contracts::GitApi;
use crate::domain::project::{ChangeKind, ChangedFile, FileChange, ProjectStatus};

pub struct GixGit;

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

impl GitApi for GixGit {
    fn status(&self, project_path: &Path) -> Result<ProjectStatus> {
        let repo = open_repo(project_path)?;
        let snap = collect_snapshot(&repo)?;
        let (ahead, behind, has_upstream) = match (snap.head_id, snap.upstream_id) {
            (Some(head), Some(upstream)) => {
                let (a, b) = count_ahead_behind(&repo, head, upstream)?;
                (a, b, true)
            }
            _ => (0, 0, false),
        };
        let changed_files = collect_changed_files(&snap);
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
            error: None,
        })
    }

    fn collect_diff(&self, project_path: &Path, staged_first: bool) -> Result<String> {
        let repo = open_repo(project_path)?;
        let snap = collect_snapshot(&repo)?;

        let staged: Vec<&FileChange> = snap
            .staged
            .iter()
            .filter(|c| c.kind != ChangeKind::Removed)
            .collect();

        let entries: Vec<&FileChange> = if staged_first && !staged.is_empty() {
            staged
        } else {
            let mut fallback: Vec<&FileChange> = snap
                .unstaged
                .iter()
                .filter(|c| c.kind != ChangeKind::Removed)
                .collect();
            fallback.extend(snap.untracked.iter());
            fallback
        };

        let mut blocks: Vec<String> = Vec::new();
        for entry in entries {
            if let Some(block) = render_file_diff(&repo, entry)? {
                blocks.push(block);
            }
        }
        if blocks.is_empty() {
            return Ok(String::new());
        }
        let joined = blocks.join("\n");
        Ok(truncate_lines(&joined, DIFF_LINE_LIMIT))
    }
}

fn open_repo(path: &Path) -> Result<gix::Repository> {
    gix::discover(path).with_context(|| format!("Не git-репозиторий: {}", path.display()))
}

fn collect_snapshot(repo: &gix::Repository) -> Result<Snapshot> {
    let head = repo.head().with_context(|| "read HEAD")?;
    let (branch, head_id) = resolve_head(repo, &head)?;

    let upstream_id = match &branch {
        Some(name) => find_upstream(repo, name)?,
        None => None,
    };

    let mut staged: Vec<FileChange> = Vec::new();
    let mut unstaged: Vec<FileChange> = Vec::new();
    let mut untracked: Vec<FileChange> = Vec::new();

    let iter = repo
        .status(gix::progress::Discard)?
        .into_iter(Vec::<BString>::new())
        .context("run git status")?;

    for item in iter {
        let item = item.context("status iteration")?;
        match item {
            gix::status::Item::TreeIndex(change) => push_tree_index(&change, &mut staged),
            gix::status::Item::IndexWorktree(inner) => {
                handle_index_worktree_item(repo, inner, &mut unstaged, &mut untracked)?;
            }
        }
    }

    staged.sort_by(|a, b| a.path.cmp(&b.path));
    unstaged.sort_by(|a, b| a.path.cmp(&b.path));
    untracked.sort_by(|a, b| a.path.cmp(&b.path));

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
) -> Result<(u32, u32)> {
    let upstream_set = reachable(repo, upstream)?;
    let head_set = reachable(repo, head)?;
    let ahead = walk_unique(repo, head, &upstream_set)?;
    let behind = walk_unique(repo, upstream, &head_set)?;
    Ok((ahead, behind))
}

fn reachable(repo: &gix::Repository, tip: gix::hash::ObjectId) -> Result<HashSet<gix::hash::ObjectId>> {
    let mut seen: HashSet<gix::hash::ObjectId> = HashSet::new();
    let mut stack = vec![tip];
    while let Some(id) = stack.pop() {
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
) -> Result<u32> {
    let mut seen: HashSet<gix::hash::ObjectId> = HashSet::new();
    let mut count: u32 = 0;
    let mut stack = vec![tip];
    while let Some(id) = stack.pop() {
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