export const state = {
  projects: [],
  statuses: {},
  // Максимальный применённый seq чтения статуса по каждому пути. Чтение с
  // меньшим seq — устаревшее (началось раньше, завершилось позже) и игнорируется.
  lastSeq: {},
  // Пути, для которых сейчас читается статус (показываем крутилку).
  loading: {},
  // Путь -> название идущей git-операции (stage/unstage/discard/commit/push).
  pending: {},
  filesOpen: new Set(),
  activeTab: 'projects',
  panelPath: null,
  panelMessage: '',
  panelSaveTimer: null,
  commitModel: '',
  commitLang: 'ru',
  commitScope: 'staged',
};

export function setProjects(projects) {
  state.projects = projects;
}

export function setStatus(path, status) {
  state.statuses[path] = status;
}

/**
 * Применяет чтение статуса от бэкенда. Возвращает false, если чтение
 * устаревшее и данные уже перекрыты более свежими.
 */
export function applySnapshot(snapshot) {
  const { path, seq, ...status } = snapshot;
  if (!path) {
    return false;
  }
  const applied = state.lastSeq[path] ?? 0;
  if (typeof seq === 'number' && seq <= applied) {
    return false;
  }
  if (typeof seq === 'number') {
    state.lastSeq[path] = seq;
  }
  state.statuses[path] = status;
  return true;
}

export function forgetProject(path) {
  delete state.statuses[path];
  delete state.lastSeq[path];
  delete state.loading[path];
  delete state.pending[path];
  state.filesOpen.delete(path);
}

export function setLoading(path, value) {
  if (value) {
    state.loading[path] = true;
  } else {
    delete state.loading[path];
  }
}

export function isLoading(path) {
  return Boolean(state.loading[path]);
}

export function setPending(path, label) {
  if (label) {
    state.pending[path] = label;
  } else {
    delete state.pending[path];
  }
}

export function pendingLabel(path) {
  return state.pending[path] || '';
}

export function isPending(path) {
  return Boolean(state.pending[path]);
}
