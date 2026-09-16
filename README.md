# repos-control

Десктоп-панель контроля git-репозиториев (Tauri 2 + Rust + Vanilla JS).

Для каждого добавленного проекта показывает:

- текущую ветку;
- staged / unstaged / untracked изменения;
- unbushed: ahead / behind относительно upstream;
- генерацию commit-сообщения по диффу (сейчас LLM-заглушка).

Git-операции выполняются библиотекой `gix` (без вызова `git.exe`).

## Сборка и запуск

- `build.bat` — dev-сборка и запуск GUI (без установщика).
- `run_check.bat` — `cargo check` через обёртку (MSVC-init + сброс sccache).

См. `AGENTS.md` — обязательные правила проекта.