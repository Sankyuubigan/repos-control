export const state = {
  projects: [],
  statuses: {},
  filesOpen: new Set(),
  activeTab: 'projects',
  busy: false,
  panelPath: null,
  panelMessage: '',
  panelSaveTimer: null,
  commitModel: '',
  commitLang: 'ru',
};

export function setProjects(projects) {
  state.projects = projects;
}

export function setStatus(path, status) {
  state.statuses[path] = status;
}