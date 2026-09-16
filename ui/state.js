export const state = {
  projects: [],
  statuses: {},
  filesOpen: new Set(),
  activeTab: 'projects',
  busy: false,
};

export function setProjects(projects) {
  state.projects = projects;
}

export function setStatus(path, status) {
  state.statuses[path] = status;
}