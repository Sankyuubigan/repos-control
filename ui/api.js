const invoke = window.__TAURI__.core.invoke;

export function listProjects() {
  return invoke('list_projects');
}

export function addProject(path) {
  return invoke('add_project', { path });
}

export function removeProject(path) {
  return invoke('remove_project', { path });
}

export function pickProjectFolder() {
  return invoke('pick_project_folder');
}

export function getProjectStatus(projectPath) {
  return invoke('get_project_status', { projectPath });
}

export function generateCommitMessage(projectPath, notes) {
  return invoke('generate_commit_message', { projectPath, notes });
}

export function logFront(msg) {
  window.__TAURI__?.logs?.logFront?.(msg);
}