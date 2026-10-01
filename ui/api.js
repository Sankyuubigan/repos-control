const invoke = window.__TAURI__.core.invoke;

const STATUS_TIMEOUT = 30000;

export function withTimeout(promise, ms, label) {
  let timer;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => {
      logFront(`${label}: таймаут (${ms / 1000}с)`);
      reject(new Error(`${label} (${ms / 1000}с)`));
    }, ms);
  });
  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer));
}

export function listProjects() {
  return invoke('list_projects');
}

export function addProject(path) {
  return invoke('add_project', { path });
}

export function removeProject(path) {
  return invoke('remove_project', { path });
}

export function reorderProjects(paths) {
  return invoke('reorder_projects', { paths });
}

export function pickProjectFolder() {
  return invoke('pick_project_folder');
}

/** Чтение статуса. Ответ — снапшот `{path, seq, ...status}`. */
export function getProjectStatus(projectPath) {
  return withTimeout(
    invoke('get_project_status', { projectPath }),
    STATUS_TIMEOUT,
    'Таймаут получения статуса',
  );
}

export function getCommitDiff(projectPath, notes, lang, scope) {
  return invoke('get_commit_diff', { projectPath, notes, lang, scope });
}

export function setCommitModel(model) {
  return invoke('set_commit_model', { model });
}

export function getCommitLang() {
  return invoke('get_commit_lang');
}

export function setCommitLang(lang) {
  return invoke('set_commit_lang', { lang });
}

/*
 * Команды записи возвращают СВЕЖИЙ статус проекта в том же ответе: запись уже
 * выполнена, состояние прочитано заново. Дополнительный запрос статуса не нужен
 * и не мог бы вернуть состояние «до операции».
 */
export function stageFiles(projectPath, paths) {
  return invoke('stage_files', { projectPath, paths });
}

export function unstageFiles(projectPath, paths) {
  return invoke('unstage_files', { projectPath, paths });
}

export function discardFiles(projectPath, paths) {
  return invoke('discard_files', { projectPath, paths });
}

export function commitChanges(projectPath, message) {
  return invoke('commit_changes', { projectPath, message });
}

export function pushChanges(projectPath) {
  return invoke('push_changes', { projectPath });
}

export function readCommitMessage(projectPath) {
  return invoke('read_commit_message', { projectPath });
}

export function writeCommitMessage(projectPath, message) {
  return invoke('write_commit_message', { projectPath, message });
}

export function logFront(msg) {
  window.__TAURI__?.logs?.logFront?.(msg);
}

export function listenStatusChanged(callback) {
  return window.__TAURI__.event.listen('status-changed', (event) => {
    callback(event.payload);
  });
}
