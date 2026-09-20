# repos-control

Десктоп-панель контроля git-репозиториев (Tauri 2 + Rust + Vanilla JS).

Для каждого добавленного проекта показывает:

- текущую ветку;
- staged / unstaged / untracked изменения;
- unbushed: ahead / behind относительно upstream;
- генерацию commit-сообщения по диффу (сейчас LLM-заглушка);
- панель коммита на проект: stage/unstage/discard файлов, сохранение сообщения в `.git/COMMIT_EDITMSG`, коммит и push.

Git-операции: чтение (статус, дифф, ahead/behind) — библиотека `gix`; запись (stage/unstage/discard/commit/push) — `git2` (libgit2, без вызова `git.exe`).

## Сборка и запуск

- `build.bat` — dev-сборка и запуск GUI (без установщика).
- `run_check.bat` — `cargo check` через обёртку (MSVC-init + сброс sccache).

См. `AGENTS.md` — обязательные правила проекта.