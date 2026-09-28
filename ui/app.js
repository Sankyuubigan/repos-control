import * as api from './api.js';
import { state, setProjects, setStatus } from './state.js';
import {
  renderProjectList,
  renderStatusSlot,
  renderFilesSlot,
  renderStatusBar,
  showModal,
  hideModal,
} from './render.js';
import { bindCommitPanelHandlers, openCommitPanel, refreshPanel, renderPanelSections } from './commitPanel.js';
import { initSettings, refreshSettings } from './settings.js';

window.__settingsRefresh = refreshSettings;

window.addEventListener('error', (event) => {
  api.logFront(`[global-error] ${event.message} @ ${event.filename}:${event.lineno}`);
});

window.addEventListener('unhandledrejection', (event) => {
  const reason = event.reason;
  const text = reason instanceof Error ? (reason.stack || reason.message) : String(reason);
  api.logFront(`[unhandledrejection] ${text}`);
});

function onStatusChanged(path, status) {
  setStatus(path, status);
  renderStatusSlot(path);
  if (state.panelPath === path) {
    renderPanelSections();
  }
}

async function refreshStatuses() {
  const paths = state.projects.map((p) => p.path);
  await Promise.all(
    paths.map(async (path) => {
      try {
        const status = await api.getProjectStatus(path);
        api.logFront(`[refreshStatuses] ${path}: ok`);
        setStatus(path, status);
        renderStatusSlot(path);
      } catch (err) {
        const message = String(err);
        api.logFront(`[refreshStatuses] ${path}: ${message}`);
        setStatus(path, { isRepo: false, error: message });
        renderStatusSlot(path);
      }
    }),
  );
}

async function refreshAll() {
  state.busy = true;
  renderStatusBar(true);
  try {
    const projects = await api.listProjects();
    setProjects(projects);
    renderProjectList();
    await refreshStatuses();
    if (state.panelPath) {
      await refreshPanel();
    }
  } catch (err) {
    api.logFront(`[refreshAll] ${String(err)}`);
  } finally {
    state.busy = false;
    renderStatusBar(false);
  }
}

async function onAddProject() {
  try {
    const path = await api.pickProjectFolder();
    if (!path) {
      return;
    }
    await api.addProject(path);
    await refreshAll();
  } catch (err) {
    showModal('Ошибка', String(err));
  }
}

async function onMoveProject(path, delta) {
  const index = state.projects.findIndex((p) => p.path === path);
  const next = index + delta;
  if (index < 0 || next < 0 || next >= state.projects.length) {
    return;
  }
  const previous = state.projects.slice();
  const [moved] = state.projects.splice(index, 1);
  state.projects.splice(next, 0, moved);
  renderProjectList();
  try {
    await api.reorderProjects(state.projects.map((p) => p.path));
  } catch (err) {
    state.projects = previous;
    renderProjectList();
    showModal('Ошибка', String(err));
  }
}

async function onRemoveProject(path) {
  if (!window.confirm(`Удалить проект «${path}» из списка?`)) {
    return;
  }
  try {
    await api.removeProject(path);
    delete state.statuses[path];
    await refreshAll();
  } catch (err) {
    showModal('Ошибка', String(err));
  }
}

async function onListClick(event) {
  const btn = event.target.closest('button[data-action]');
  if (!btn) {
    return;
  }
  const path = btn.dataset.path;
  switch (btn.dataset.action) {
    case 'move-up':
      onMoveProject(path, -1);
      break;
    case 'move-down':
      onMoveProject(path, 1);
      break;
    case 'open-panel':
      openCommitPanel(path);
      break;
    case 'remove':
      onRemoveProject(path);
      break;
    case 'toggle-files':
      if (state.filesOpen.has(path)) {
        state.filesOpen.delete(path);
      } else {
        state.filesOpen.add(path);
      }
      renderFilesSlot(path);
      break;
  }
}

function onTopBarClick(event) {
  switch (event.target.id) {
    case 'btn-refresh':
      refreshAll();
      break;
    case 'btn-add':
      onAddProject();
      break;
  }
}

function onTabClick(event) {
  const btn = event.target.closest('.tab-btn');
  if (!btn || btn.classList.contains('active')) {
    return;
  }
  const tab = btn.dataset.tab;
  state.activeTab = tab;
  document.querySelectorAll('.tab-btn').forEach((b) => {
    b.classList.toggle('active', b.dataset.tab === tab);
  });
  document.querySelectorAll('.tab-pane').forEach((pane) => {
    pane.classList.toggle('hidden', pane.id !== `tab-${tab}`);
  });
  if (tab === 'settings') {
    window.__settingsRefresh?.();
  }
}

function onModalClick(event) {
  switch (event.target.id) {
    case 'btn-close':
    case 'btn-copy': {
      if (event.target.id === 'btn-copy') {
        const body = document.getElementById('modal-body').textContent;
        navigator.clipboard?.writeText(body).catch(() => {});
      }
      hideModal();
      break;
    }
  }
}

document.addEventListener('DOMContentLoaded', async () => {
  document.getElementById('project-list').addEventListener('click', onListClick);
  document.querySelector('.topbar-actions').addEventListener('click', onTopBarClick);
  document.querySelector('.tabbar').addEventListener('click', onTabClick);
  bindCommitPanelHandlers();
  document.getElementById('modal-backdrop').addEventListener('click', (event) => {
    if (event.target === event.currentTarget) {
      hideModal();
    }
  });
  document.getElementById('modal-actions').addEventListener('click', onModalClick);

  api.listenStatusChanged((payload) => {
    const { path, ...status } = payload;
    onStatusChanged(path, status);
  });

  initSettings();

  await refreshAll();
});