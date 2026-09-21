use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::domain::contracts::GitWriteApi;

pub struct Git2Write;

impl GitWriteApi for Git2Write {
    fn stage(&self, project_path: &Path, paths: &[String]) -> Result<()> {
        let repo = open_repo(project_path)?;
        let workdir = repo
            .workdir()
            .context("у репозитория нет рабочего дерева")?
            .to_path_buf();
        let mut index = repo.index().context("открыть индекс")?;
        for path in paths {
            stage_path(&mut index, &workdir, Path::new(path))?;
        }
        index.write().context("записать индекс")?;
        log::info!("stage: {} файл(ов) в {}", paths.len(), project_path.display());
        Ok(())
    }

    fn unstage(&self, project_path: &Path, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let repo = open_repo(project_path)?;
        repo.reset_default(None, paths.iter().cloned())
            .context("снять файлы с индекса (reset)")?;
        log::info!("unstage: {} файл(ов) в {}", paths.len(), project_path.display());
        Ok(())
    }

    fn discard(&self, project_path: &Path, paths: &[String]) -> Result<()> {
        let repo = open_repo(project_path)?;
        let workdir = repo
            .workdir()
            .context("у репозитория нет рабочего дерева")?
            .to_path_buf();
        for path in paths {
            let status = repo
                .status_file(Path::new(path))
                .with_context(|| format!("status файла {path}"))?;
            if status.contains(git2::Status::WT_NEW) {
                remove_worktree_entry(&workdir, path)?;
            } else {
                let mut opts = git2::build::CheckoutBuilder::new();
                opts.force().path(path.as_str());
                repo.checkout_head(Some(&mut opts))
                    .with_context(|| format!("откатить изменения {path}"))?;
            }
        }
        log::info!("discard: {} файл(ов) в {}", paths.len(), project_path.display());
        Ok(())
    }

    fn commit(&self, project_path: &Path, message: &str) -> Result<()> {
        let repo = open_repo(project_path)?;
        let signature = repo
            .signature()
            .context("git config user.name / user.email не заданы")?;
        let mut index = repo.index().context("открыть индекс")?;
        let tree_id = index.write_tree().context("записать дерево индекса")?;
        let tree = repo.find_tree(tree_id).context("найти дерево")?;

        let parents = match repo.head() {
            Ok(head) => head
                .target()
                .map(|oid| repo.find_commit(oid))
                .transpose()
                .context("найти HEAD-коммит")?
                .into_iter()
                .collect::<Vec<_>>(),
            Err(err) if err.code() == git2::ErrorCode::UnbornBranch => Vec::new(),
            Err(err) => return Err(anyhow::Error::new(err).context("прочитать HEAD")),
        };
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();

        let oid = repo
            .commit(Some("HEAD"), &signature, &signature, message, &tree, &parent_refs)
            .context("создать коммит")?;
        log::info!(
            "commit {oid} в {}: {}",
            project_path.display(),
            first_line(message)
        );
        Ok(())
    }

    fn push(&self, project_path: &Path) -> Result<()> {
        let repo = open_repo(project_path)?;
        let head = match repo.head() {
            Ok(head) => head,
            Err(err) if err.code() == git2::ErrorCode::UnbornBranch => {
                bail!("нет коммитов для отправки")
            }
            Err(err) => return Err(anyhow::Error::new(err).context("прочитать HEAD")),
        };
        let branch = head
            .shorthand()
            .context("HEAD не указывает на ветку")?
            .to_string();
        let mut remote = repo
            .find_remote("origin")
            .context("remote 'origin' не найден")?;
        let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");

        let mut callbacks = git2::RemoteCallbacks::new();
        callbacks.credentials(credentials);
        callbacks.push_update_reference(|name, status| match status {
            Some(msg) => {
                log::error!("push отклонён {name}: {msg}");
                Err(git2::Error::from_str(msg))
            }
            None => {
                log::info!("push обновил {name}");
                Ok(())
            }
        });

        let mut opts = git2::PushOptions::new();
        opts.remote_callbacks(callbacks);
        remote
            .push(&[refspec.as_str()], Some(&mut opts))
            .with_context(|| format!("push ветки {branch} в origin"))?;
        log::info!("push {branch} -> origin в {}", project_path.display());
        Ok(())
    }
}

fn open_repo(path: &Path) -> Result<git2::Repository> {
    git2::Repository::open(path)
        .with_context(|| format!("открыть git-репозиторий: {}", path.display()))
}

fn stage_path(index: &mut git2::Index, workdir: &Path, path: &Path) -> Result<()> {
    if workdir.join(path).exists() {
        index
            .add_path(path)
            .with_context(|| format!("добавить в индекс: {}", path.display()))?;
        return Ok(());
    }
    if index.get_path(path, 0).is_some() {
        // Файл удалён с диска, но есть в индексе — стадим удаление,
        // как это делает `git add` для трекаемого удалённого файла.
        index
            .remove_path(path)
            .with_context(|| format!("удалить из индекса: {}", path.display()))?;
        log::info!("stage (удаление): {}", path.display());
        return Ok(());
    }
    bail!(
        "файл не найден на диске и отсутствует в индексе: {}",
        path.display()
    )
}

fn remove_worktree_entry(workdir: &Path, rel: &str) -> Result<()> {
    let target = workdir.join(rel);
    if !target.starts_with(workdir) {
        bail!("путь вне рабочего дерева: {rel}");
    }
    if target.is_dir() {
        fs::remove_dir_all(&target)
            .with_context(|| format!("удалить каталог {}", target.display()))?;
    } else if target.exists() {
        fs::remove_file(&target).with_context(|| format!("удалить файл {}", target.display()))?;
    }
    log::info!("удалён untracked {}", target.display());
    Ok(())
}

fn first_line(message: &str) -> &str {
    message.lines().next().unwrap_or("").trim()
}

fn credentials(
    url: &str,
    username: Option<&str>,
    allowed: git2::CredentialType,
) -> Result<git2::Cred, git2::Error> {
    if allowed.contains(git2::CredentialType::SSH_KEY) {
        let user = username.unwrap_or("git");
        if let Ok(cred) = git2::Cred::ssh_key_from_agent(user) {
            log::info!("push auth: ssh-agent ({user})");
            return Ok(cred);
        }
        if let Some(key) = find_ssh_key() {
            log::info!("push auth: ssh-ключ {}", key.display());
            return git2::Cred::ssh_key(user, None, &key, None);
        }
        log::warn!("push auth: ssh-agent и ключи ~/.ssh недоступны для {url}");
    }
    if allowed.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
        let config = git2::Config::open_default()?;
        return git2::Cred::credential_helper(&config, url, username);
    }
    if allowed.contains(git2::CredentialType::USERNAME) {
        return git2::Cred::username(username.unwrap_or("git"));
    }
    git2::Cred::default()
}

fn find_ssh_key() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let ssh_dir = PathBuf::from(home).join(".ssh");
    ["id_ed25519", "id_rsa", "id_ecdsa"]
        .iter()
        .map(|name| ssh_dir.join(name))
        .find(|path| path.exists())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempRepo {
        path: PathBuf,
    }

    impl TempRepo {
        fn new() -> TempRepo {
            let unique = format!(
                "repos-control-git2-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            );
            let path = std::env::temp_dir().join(unique);
            fs::create_dir_all(&path).expect("create temp dir");
            TempRepo { path }
        }

        fn repo(&self) -> git2::Repository {
            let repo = git2::Repository::init(&self.path).expect("init repo");
            let mut config = repo.config().expect("config");
            config.set_str("user.name", "Test").expect("user.name");
            config.set_str("user.email", "test@example.com").expect("user.email");
            repo
        }

        fn write(&self, name: &str, content: &str) {
            fs::write(self.path.join(name), content).expect("write file");
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn paths(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn stage_then_unstage_roundtrip() {
        let tmp = TempRepo::new();
        let repo = tmp.repo();
        tmp.write("a.txt", "hello");
        let write = Git2Write;

        write.stage(&tmp.path, &paths(&["a.txt"])).expect("stage");
        let status = repo.status_file(Path::new("a.txt")).expect("status");
        assert!(status.contains(git2::Status::INDEX_NEW));

        write.unstage(&tmp.path, &paths(&["a.txt"])).expect("unstage");
        let status = repo.status_file(Path::new("a.txt")).expect("status");
        assert!(status.contains(git2::Status::WT_NEW));
        assert!(!status.contains(git2::Status::INDEX_NEW));
    }

    #[test]
    fn commit_creates_head() {
        let tmp = TempRepo::new();
        let repo = tmp.repo();
        tmp.write("a.txt", "hello");
        let write = Git2Write;
        write.stage(&tmp.path, &paths(&["a.txt"])).expect("stage");
        write.commit(&tmp.path, "first commit").expect("commit");

        let head = repo.head().expect("head");
        assert!(head.target().is_some());
    }

    #[test]
    fn discard_removes_untracked() {
        let tmp = TempRepo::new();
        tmp.repo();
        tmp.write("new.txt", "data");
        let write = Git2Write;
        write.discard(&tmp.path, &paths(&["new.txt"])).expect("discard");
        assert!(!tmp.path.join("new.txt").exists());
    }

    #[test]
    fn discard_reverts_tracked_changes() {
        let tmp = TempRepo::new();
        let repo = tmp.repo();
        tmp.write("a.txt", "original");
        let write = Git2Write;
        write.stage(&tmp.path, &paths(&["a.txt"])).expect("stage");
        write.commit(&tmp.path, "init").expect("commit");
        tmp.write("a.txt", "modified");
        let status = repo.status_file(Path::new("a.txt")).expect("status");
        assert!(status.contains(git2::Status::WT_MODIFIED));

        write.discard(&tmp.path, &paths(&["a.txt"])).expect("discard");
        let content = fs::read_to_string(tmp.path.join("a.txt")).expect("read");
        assert_eq!(content, "original");
    }

    #[test]
    fn stage_records_deletion_of_removed_tracked_file() {
        let tmp = TempRepo::new();
        let repo = tmp.repo();
        tmp.write("a.txt", "hello");
        let write = Git2Write;
        write.stage(&tmp.path, &paths(&["a.txt"])).expect("stage");
        write.commit(&tmp.path, "init").expect("commit");
        fs::remove_file(tmp.path.join("a.txt")).expect("remove file");

        write.stage(&tmp.path, &paths(&["a.txt"])).expect("stage deletion");
        let status = repo.status_file(Path::new("a.txt")).expect("status");
        assert!(status.contains(git2::Status::INDEX_DELETED));

        write.commit(&tmp.path, "remove a").expect("commit deletion");
        let index = repo.index().expect("index");
        assert!(
            index.get_path(Path::new("a.txt"), 0).is_none(),
            "после коммита удаления файла не должно быть в индексе"
        );
    }

    #[test]
    fn stage_missing_untracked_path_errors() {
        let tmp = TempRepo::new();
        tmp.repo();
        let write = Git2Write;
        let err = write
            .stage(&tmp.path, &paths(&["ghost.txt"]))
            .expect_err("stage must fail");
        assert!(
            err.to_string().contains("ghost.txt"),
            "ошибка обязана называть путь: {err}"
        );
    }
}
