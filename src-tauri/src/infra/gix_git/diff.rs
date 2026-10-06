use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use gix::diff::blob::unified_diff::{ConsumeBinaryHunk, ContextSize};
use gix::diff::blob::{diff_with_slider_heuristics, Algorithm, InternedInput, UnifiedDiff};

use crate::domain::contracts::DiffScope;
use crate::domain::project::{ChangeKind, FileChange};

use super::snapshot::Snapshot;

const DIFF_CONTEXT: u32 = 3;
const BINARY_SNOOP: usize = 8000;

pub(super) fn collect_diff(
    repo: &gix::Repository,
    snap: &Snapshot,
    scope: DiffScope,
    interrupt: &Arc<AtomicBool>,
    deadline: Instant,
) -> Result<String> {
    let entries = entries_for_scope(snap, scope);
    let mut blocks: Vec<String> = Vec::new();
    for entry in entries {
        super::check_deadline(interrupt, deadline, "diff")?;
        if let Some(block) = render_file_diff(repo, entry)? {
            blocks.push(block);
        }
    }
    if blocks.is_empty() {
        return Ok(String::new());
    }
    Ok(blocks.join("\n"))
}

fn entries_for_scope<'a>(snap: &'a Snapshot, scope: DiffScope) -> Vec<&'a FileChange> {
    match scope {
        DiffScope::Staged => snap.staged.iter().collect(),
        DiffScope::Unstaged => {
            let mut v: Vec<&FileChange> = snap.unstaged.iter().collect();
            v.extend(snap.untracked.iter());
            v
        }
        DiffScope::All => {
            let mut v: Vec<&FileChange> = snap.staged.iter().collect();
            v.extend(snap.unstaged.iter());
            v.extend(snap.untracked.iter());
            v
        }
    }
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
    if entry.kind == ChangeKind::Removed {
        return Ok(Vec::new());
    }
    let Some(path) = entry.worktree.as_deref() else {
        return Ok(Vec::new());
    };
    match std::fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            log::warn!("worktree file vanished mid-diff: {}", path.display());
            Ok(Vec::new())
        }
        Err(err) => {
            Err(anyhow::Error::new(err).context(format!("read worktree file {}", path.display())))
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_detected_on_null_byte() {
        assert!(looks_binary(&[1, 0, 2]));
        assert!(!looks_binary(b"plain text"));
    }
}
