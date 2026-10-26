use std::collections::{BinaryHeap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};

use crate::domain::project::CommitEntry;

use super::lossy;

/// Элемент очереди обхода. Куча — максимальная, поэтому первым всегда
/// извлекается коммит с самым свежим временем коммиттера.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Queued {
    time: i64,
    seq: u64,
    id: gix::hash::ObjectId,
}

/// Последние `limit` коммитов от HEAD, новейшие сверху.
///
/// Куча вместо полного обхода истории: в очередь кладутся только предки уже
/// показанных коммитов, поэтому читается ровно столько объектов, сколько нужно
/// для `limit` строк, — вся история огромного репозитория не трогается.
pub(super) fn list(
    repo: &gix::Repository,
    limit: usize,
    interrupt: &Arc<AtomicBool>,
    deadline: Instant,
) -> Result<Vec<CommitEntry>> {
    let head = repo.head().context("read HEAD")?;
    if head.is_unborn() {
        return Ok(Vec::new());
    }
    let tip = repo.head_id().context("resolve HEAD id")?.detach();

    let mut out: Vec<CommitEntry> = Vec::new();
    let mut seen: HashSet<gix::hash::ObjectId> = HashSet::new();
    let mut queue: BinaryHeap<Queued> = BinaryHeap::new();
    let mut seq: u64 = 0;
    // Время HEAD заранее неизвестно, но он обязан быть первым: i64::MAX
    // гарантирует извлечение из кучи до любых предков.
    queue.push(Queued { time: i64::MAX, seq, id: tip });

    while out.len() < limit {
        super::check_deadline(interrupt, deadline, "history")?;
        let Some(next) = queue.pop() else {
            break;
        };
        if !seen.insert(next.id) {
            continue;
        }
        let commit = repo
            .find_commit(next.id)
            .with_context(|| format!("find commit {}", next.id))?;
        let time = commit
            .time()
            .with_context(|| format!("read time of {}", next.id))?
            .seconds;
        out.push(CommitEntry {
            short_id: short_id(next.id),
            message: summary(&commit),
            time,
            is_head: next.id == tip,
        });
        for parent in commit.parent_ids() {
            let pid = parent.detach();
            if seen.contains(&pid) {
                continue;
            }
            seq += 1;
            let ptime = repo
                .find_commit(pid)
                .with_context(|| format!("find parent {pid}"))?
                .time()
                .with_context(|| format!("read time of {pid}"))?
                .seconds;
            queue.push(Queued { time: ptime, seq, id: pid });
        }
    }
    Ok(out)
}

fn short_id(id: gix::hash::ObjectId) -> String {
    let hex = id.to_hex().to_string();
    hex.chars().take(7).collect()
}

/// Первая строка сообщения коммита (subject), как в `git log --oneline`.
/// Нечитаемое сообщение не роняет список: показываем сырые байты через lossy.
fn summary(commit: &gix::Commit<'_>) -> String {
    let raw = commit
        .message()
        .ok()
        .map(|m| m.summary().into_owned())
        .unwrap_or_else(|| commit.message_raw_sloppy().to_owned());
    let text = lossy(&raw);
    let first_line = text.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        "(без сообщения)".to_string()
    } else {
        first_line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;

    const TIMEOUT: Duration = Duration::from_secs(15);

    fn temp_repo(name: &str) -> git2::Repository {
        let dir = std::env::temp_dir().join(format!(
            "repos-control-history-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git2::Repository::init(&dir).unwrap()
    }

    fn commit_file(repo: &git2::Repository, name: &str, message: &str) -> git2::Oid {
        std::fs::write(repo.workdir().unwrap().join(name), format!("{message}\n")).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new(name)).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("tester", "tester@example.com").unwrap();
        let parent = repo
            .head()
            .ok()
            .and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .unwrap()
    }

    fn open(workdir: &std::path::Path) -> gix::Repository {
        // tests -> history -> gix_git: на два уровня вверх, к sibling-модулю snapshot.
        super::super::snapshot::open_repo(workdir).unwrap()
    }

    fn run(repo: &gix::Repository, limit: usize) -> Vec<CommitEntry> {
        let interrupt = Arc::new(AtomicBool::new(false));
        list(repo, limit, &interrupt, Instant::now() + TIMEOUT).unwrap()
    }

    #[test]
    fn history_lists_newest_first_and_marks_head() {
        let repo = temp_repo("newest-first");
        commit_file(&repo, "a.txt", "первый коммит");
        commit_file(&repo, "b.txt", "второй коммит");
        commit_file(&repo, "c.txt", "третий коммит");

        let entries = run(&open(repo.workdir().unwrap()), 10);
        let messages: Vec<&str> = entries.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(
            messages,
            vec!["третий коммит", "второй коммит", "первый коммит"],
            "новейший коммит обязан быть первым"
        );
        assert!(entries[0].is_head, "HEAD обязан быть отмечен");
        assert!(!entries[1].is_head);
        assert!(entries[0].time >= entries[2].time, "время не убывает");
        assert_eq!(entries[0].short_id.len(), 7);
    }

    #[test]
    fn history_of_unborn_repo_is_empty() {
        let repo = temp_repo("unborn");
        assert!(run(&open(repo.workdir().unwrap()), 10).is_empty());
    }

    #[test]
    fn history_respects_limit() {
        let repo = temp_repo("limit");
        commit_file(&repo, "a.txt", "коммит 1");
        commit_file(&repo, "b.txt", "коммит 2");
        commit_file(&repo, "c.txt", "коммит 3");

        let entries = run(&open(repo.workdir().unwrap()), 2);
        let messages: Vec<&str> = entries.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(messages, vec!["коммит 3", "коммит 2"], "берутся новейшие");
    }

    /// Subject из многострочного сообщения — только первая строка.
    #[test]
    fn history_takes_subject_from_multiline_message() {
        let repo = temp_repo("multiline");
        commit_file(&repo, "a.txt", "заголовок\n\nподробное описание тела");
        let entries = run(&open(repo.workdir().unwrap()), 10);
        assert_eq!(entries[0].message, "заголовок");
    }
}
