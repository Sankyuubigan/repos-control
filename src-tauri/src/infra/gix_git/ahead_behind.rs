use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};

pub(super) fn count(
    repo: &gix::Repository,
    head: gix::hash::ObjectId,
    upstream: gix::hash::ObjectId,
    interrupt: &Arc<AtomicBool>,
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
    interrupt: &Arc<AtomicBool>,
    deadline: Instant,
) -> Result<HashSet<gix::hash::ObjectId>> {
    let mut seen: HashSet<gix::hash::ObjectId> = HashSet::new();
    let mut stack = vec![tip];
    while let Some(id) = stack.pop() {
        super::check_deadline(interrupt, deadline, "ahead_behind")?;
        if !seen.insert(id) {
            continue;
        }
        let commit = repo
            .find_commit(id)
            .with_context(|| format!("find commit {id}"))?;
        stack.extend(commit.parent_ids().map(|p| p.detach()));
    }
    Ok(seen)
}

fn walk_unique(
    repo: &gix::Repository,
    tip: gix::hash::ObjectId,
    shared: &HashSet<gix::hash::ObjectId>,
    interrupt: &Arc<AtomicBool>,
    deadline: Instant,
) -> Result<u32> {
    let mut seen: HashSet<gix::hash::ObjectId> = HashSet::new();
    let mut count: u32 = 0;
    let mut stack = vec![tip];
    while let Some(id) = stack.pop() {
        super::check_deadline(interrupt, deadline, "ahead_behind")?;
        if shared.contains(&id) {
            continue;
        }
        if !seen.insert(id) {
            continue;
        }
        count += 1;
        let commit = repo
            .find_commit(id)
            .with_context(|| format!("find commit {id}"))?;
        stack.extend(commit.parent_ids().map(|p| p.detach()));
    }
    Ok(count)
}
