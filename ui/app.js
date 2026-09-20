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
import { bindCommitPanelHandlers, openCommitPanel, refreshPanel } from './commitPanel.js';

async function refreshStatuses() {
  const paths = state.projects.map((p) => p.path);
  await Promise.all(
    paths.map(async (path) => {
      try {
        const status = await api.getProjectStatus(path);
        setStatus(path, status);
        renderStatusSlot(path);
      } catch (err) {
        setStatus(path, { isRepo: false, error: String(err) });
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

async function onGenerate(path) {
  const notes = window.prompt('Заметки разработчика (необязательно, игнорируются если нерелевантно):', '');
  if (notes === null) {
    return;
  }
  try {
    const message = await api.generateCommitMessage(path, notes);
    showModal('Commit-сообщение', message);
  } catch (err) {
    showModal('Ошибка', String(err));
  }
}

function onListClick(event) {
  const btn = event.target.closest('button[data-action]');
  if (!btn) {
    return;
  }
  const path = btn.dataset.path;
  switch (btn.dataset.action) {
    case 'generate':
      onGenerate(path);
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
  document.getElementById('tab-projects').classList.toggle('hidden', tab !== 'projects');
  document.getElementById('tab-logs').classList.toggle('hidden', tab !== 'logs');
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

  await refreshAll();
});