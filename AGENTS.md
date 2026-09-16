# AGENTS.md - Правила проекта repos-control

## ОБЯЗАТЕЛЬНО: Чтение документации

Перед любой работой над этим проектом ОБЯЗАТЕЛЬНО прочитать документацию из:

1. `D:\Projects\docusaurus-starter\docs\Sega Mega Note\Моя картотека\software\настройки\global_ai_docs\core\rules.md` — базовые правила (универсальные)
2. `D:\Projects\docusaurus-starter\docs\Sega Mega Note\Моя картотека\software\настройки\global_ai_docs\desktop_rust_tauri\rules.md` — правила для Rust + Tauri

## Краткая сводка ключевых правил

### Проект

- **Назначение**: десктоп (Tauri 2) панель контроля git-репозиториев. Показывает для каждого добавленного проекта: ветку, staged/unstaged/untracked изменения, unpushed (ahead/behind) и умеет генерировать commit-сообщение по диффу (LLM — заглушка).
- **Стек**: Rust (backend) + Tauri 2 + Vanilla JS (frontend-статика в `ui/`, без npm/бандлера). Git-операции — библиотека `gix` (без вызова git CLI).
- **Архитектура**: `api/` (тонкие Tauri-команды) -> `domain/` (бизнес-логика, трейты-контракты, промпт, usecases) -> `infra/` (реализации: gix, JSON-хранилище, LLM-заглушка). Двери модулей — `mod.rs`. Трейты живут в `domain/contracts.rs`.
- **Конфиг приложения**: `%APPDATA%\com.reposcontrol.app\app_config.json` (список проектов).
- **Сборка и запуск — только через плагин `tauri-build-toolkit`** (`D:\Projects\my-tauri-plugins\tauri-build-toolkit`): корневой `build.bat` вызывает `node cli.cjs build` (бамп версии YY.M.P + `npx tauri build` + detached-запуск приложения). Рукопашный `start exe`, прямой `cargo build/test/check`, `npx tauri` — запрещены. Dev-режим недопустим: сборка всегда production (вшитые ассеты), `devUrl` в `tauri.conf.json` удалён. Проверка — `run_check.bat` (`cargo check` + `cargo test --lib` с `--features custom-protocol`); тесты также через `test.bat`.

### Обязательные исполнения

1. Сборка/проверка только через `.bat`-обёртки (MSVC init, сброс sccache-обёрток).
2. Git-команды (commit/push/pull/...) — только с явного письменного разрешения пользователя.
3. Логи: вкладка «Логи» в GUI + файл `test/last_logs.txt` (пересоздаётся при старте). Каждая строка лога с меткой `[ГГГГ-ММ-ДД ЧЧ:ММ:СС]`. Единая точка записи — кастомный логгер `src-tauri/src/logging.rs`.
4. Файлы — не больше 300-500 строк. `unwrap()`/`expect()` вне тестов запрещены.
5. Ошибки нельзя глотать молча: `warn!`/`error!` в лог, либо `Result`, либо сообщение в UI. Никаких `.ok()?` без логирования.
6. Subprocess на Windows — `CREATE_NO_WINDOW` (`creation_flags(0x08000000)`).
7. GUI-subsystem всегда: `#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]`.
8. Крупные задачи — файл-план в `tasks/` (чек-боксы, вести статус).
9. Rust-классика: `?` + `anyhow`, функции короче 50-80 строк, без комментариев-мусора.