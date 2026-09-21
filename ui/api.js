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

export function reorderProjects(paths) {
  return invoke('reorder_projects', { paths });
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