use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Путь к .gitignore в корне репозитория.
pub fn gitignore_path(project_path: &Path) -> PathBuf {
    project_path.join(".gitignore")
}

/// Возвращает путь к .gitignore, создав пустой файл, если его ещё нет.
pub fn ensure_gitignore(project_path: &Path) -> Result<PathBuf> {
    let path = gitignore_path(project_path);
    if path.exists() {
        return Ok(path);
    }
    fs::write(&path, b"").with_context(|| format!("создать {}", path.display()))?;
    log::info!("создан пустой .gitignore: {}", path.display());
    Ok(path)
}

/// Добавляет запись о файле в .gitignore. Путь якорится к корню репозитория
/// (`/путь/к/файлу`), чтобы игнорировался именно этот файл, а не одноимённые
/// файлы в других папках. Повторное добавление дубля не создаёт.
pub fn add_entry(project_path: &Path, file_path: &str) -> Result<()> {
    let entry = anchored_entry(file_path);
    let path = ensure_gitignore(project_path)?;
    let existing = fs::read_to_string(&path)
        .with_context(|| format!("прочитать {}", path.display()))?;
    if existing.lines().any(|line| line.trim() == entry) {
        log::info!(".gitignore уже содержит {entry}: {}", path.display());
        return Ok(());
    }
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .with_context(|| format!("открыть для записи {}", path.display()))?;
    if !existing.is_empty() && !existing.ends_with('\n') {
        file.write_all(b"\n")
            .with_context(|| format!("записать в {}", path.display()))?;
    }
    file.write_all(entry.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .with_context(|| format!("записать в {}", path.display()))?;
    log::info!("добавлено в {}: {entry}", path.display());
    Ok(())
}

/// Открывает .gitignore репозитория в Блокноте (создаёт пустой файл при
/// отсутствии, чтобы пользователь сразу мог его наполнить).
pub fn open_in_notepad(project_path: &Path) -> Result<()> {
    let path = ensure_gitignore(project_path)?;
    let mut cmd = std::process::Command::new("notepad.exe");
    cmd.arg(&path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: notepad — GUI-процесс без консоли, флаг гасит
        // возможный консольный мерцающий хост запуска (правила 6.8).
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.spawn()
        .with_context(|| format!("запустить блокнот для {}", path.display()))?;
    log::info!(".gitignore открыт в блокноте: {}", path.display());
    Ok(())
}

fn anchored_entry(file_path: &str) -> String {
    let normalized = file_path.replace('\\', "/");
    if normalized.starts_with('/') {
        normalized
    } else {
        format!("/{normalized}")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::domain::contracts::GitApi;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(name: &str) -> TempDir {
            let unique = format!(
                "repos-control-gitignore-{name}-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            );
            let path = std::env::temp_dir().join(unique);
            fs::create_dir_all(&path).expect("create temp dir");
            TempDir { path }
        }

        fn read_gitignore(&self) -> String {
            fs::read_to_string(self.path.join(".gitignore")).expect("read .gitignore")
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn add_entry_creates_gitignore_with_anchored_path() {
        let tmp = TempDir::new("create");
        add_entry(&tmp.path, "build/output.log").expect("add entry");
        assert_eq!(tmp.read_gitignore(), "/build/output.log\n");
    }

    #[test]
    fn add_entry_backslashes_are_normalized() {
        let tmp = TempDir::new("backslash");
        add_entry(&tmp.path, "sub\\file.txt").expect("add entry");
        assert_eq!(tmp.read_gitignore(), "/sub/file.txt\n");
    }

    #[test]
    fn add_entry_does_not_duplicate() {
        let tmp = TempDir::new("dedup");
        add_entry(&tmp.path, "notes.txt").expect("первое добавление");
        add_entry(&tmp.path, "notes.txt").expect("второе добавление");
        assert_eq!(tmp.read_gitignore(), "/notes.txt\n");
    }

    #[test]
    fn add_entry_appends_after_line_without_trailing_newline() {
        let tmp = TempDir::new("append");
        fs::write(tmp.path.join(".gitignore"), "node_modules").expect("seed gitignore");
        add_entry(&tmp.path, "sub/file.txt").expect("add entry");
        assert_eq!(tmp.read_gitignore(), "node_modules\n/sub/file.txt\n");
    }

    /// Регрессия: после добавления в .gitignore файл обязан исчезнуть из
    /// untracked-списка статуса сразу, без дополнительных действий.
    #[test]
    fn status_hides_ignored_untracked_file() {
        let tmp = TempDir::new("status");
        git2::Repository::init(&tmp.path).expect("init repo");
        fs::write(tmp.path.join("noise.tmp"), "x").expect("write file");

        let gix = crate::infra::GixGit::new();
        let before = gix.status(&tmp.path).expect("status до");
        assert_eq!(before.untracked, 1, "файл ещё не игнорируется");

        add_entry(&tmp.path, "noise.tmp").expect("add entry");
        let after = gix.status(&tmp.path).expect("status после");
        // Созданный add_entry .gitignore сам становится untracked — это
        // обычное поведение git, поэтому в списке остаётся ровно он.
        assert_eq!(after.untracked, 1, "остаётся только новый .gitignore");
        assert!(
            after
                .unstaged_files
                .iter()
                .all(|file| file.path == ".gitignore"),
            "noise.tmp обязан исчезнуть из untracked, остались: {:?}",
            after.unstaged_files
        );
    }
}
