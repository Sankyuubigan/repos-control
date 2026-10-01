import { state, isLoading, isPending } from './state.js';
import { fileIconHtml } from './fileIcons.js';

const listEl = document.getElementById('project-list');
const emptyEl = document.getElementById('empty-state');

export function esc(text) {
  const div = document.createElement('div');
  div.textContent = String(text);
  return div.innerHTML;
}

function badge(label, cls, title, muted) {
  const extra = muted ? ' badge-muted' : '';
  return `<span class="badge badge-${cls}${extra}" title="${esc(title)}">${esc(label)}</span>`;
}

function renderStatus(path) {
  const busy = isLoading(path) || isPending(path);
  const spin = busy ? '<span class="card-spinner"></span>' : '';
  const st = state.statuses[path];
  if (!st) {
    return `${spin}${badge('…', 'branch', 'загрузка')}`;
  }
  if (!st.isRepo) {
    return `${spin}<div class="error-banner">${esc(st.error || 'Репозиторий недоступен')}</div>`;
  }
  const badges = [];
  badges.push(badge(st.branch || '(нет ветки)', 'branch', `ветка: ${st.branch}`));
  badges.push(badge(`A ${st.staged}`, 'staged', 'staged (проиндексировано)', st.staged === 0));
  badges.push(badge(`M ${st.unstaged}`, 'unstaged', 'unstaged (изменено)', st.unstaged === 0));
  badges.push(badge(`U ${st.untracked}`, 'untracked', 'untracked (не отслеживается)', st.untracked === 0));
  if (st.hasUpstream) {
    badges.push(badge(`ahead ${st.ahead}`, 'ahead', 'не запушено', st.ahead === 0));
    badges.push(badge(`behind ${st.behind}`, 'behind', 'пришли чужие коммиты', st.behind === 0));
  } else {
    badges.push(badge('нет upstream', 'behind', 'не настроен upstream-remote'));
  }
  if (st.error) {
    badges.push(`<span class="error-banner">${esc(st.error)}</span>`);
  }
  return `${spin}${badges.join('')}`;
}

export function projectName(path) {
  const trimmed = String(path).replace(/[\\/]+$/, '');
  const i = Math.max(trimmed.lastIndexOf('\\'), trimmed.lastIndexOf('/'));
  return i >= 0 ? trimmed.slice(i + 1) : trimmed;
}

function projectCard(project, index) {
  const path = project.path;
  const first = index === 0;
  const last = index === state.projects.length - 1;
  return `<div class="project-card" data-path="${esc(path)}">
    <div class="project-head">
      <div>
        <div class="project-name">${esc(projectName(path))}</div>
        <div class="project-path" title="${esc(path)}">${esc(path)}</div>
      </div>
      <div class="project-actions">
        <div class="project-move">
          <button class="btn btn-tiny" data-action="move-up" data-path="${esc(path)}" title="Выше" ${first ? 'disabled' : ''}>↑</button>
          <button class="btn btn-tiny" data-action="move-down" data-path="${esc(path)}" title="Ниже" ${last ? 'disabled' : ''}>↓</button>
        </div>
        <button class="btn btn-generation" data-action="open-panel" data-path="${esc(path)}" title="Открыть панель коммита">Панель коммита</button>
        <button class="btn" data-action="remove" data-path="${esc(path)}">Удалить</button>
      </div>
    </div>
    <div class="badges" data-slot="status">${renderStatus(path)}</div>
    <div data-slot="files">${renderFiles(path)}</div>
  </div>`;
}

function isFilesOpen(path) {
  return state.filesOpen.has(path);
}

function renderFiles(path) {
  const st = state.statuses[path];
  const files = st && Array.isArray(st.changedFiles) ? st.changedFiles : [];
  const open = isFilesOpen(path);
  const arrow = open ? '▴' : '▾';
  const items = files
    .map((f) => {
      const s = String(f.status).toUpperCase();
      const deleted = s === 'D' ? ' is-deleted' : '';
      return `<div class="file-row">${fileIconHtml(f.path)}<span class="file-path-item${deleted}">${esc(f.path)}</span><span class="file-status file-status-${esc(s)}">${esc(s)}</span></div>`;
    })
    .join('');
  return `<div class="files-block">
    <button class="btn btn-small btn-files-toggle" data-action="toggle-files" data-path="${esc(path)}">Файлы (${files.length}) ${arrow}</button>
    <div class="files-list ${open ? '' : 'hidden'}">${items}</div>
  </div>`;
}

export function renderProjectList() {
  if (state.projects.length === 0) {
    listEl.innerHTML = '';
    emptyEl.classList.remove('hidden');
    return;
  }
  emptyEl.classList.add('hidden');
  listEl.innerHTML = state.projects.map((project, index) => projectCard(project, index)).join('');
}

export function renderStatusSlot(path) {
  const cards = listEl.querySelectorAll('.project-card');
  for (const card of cards) {
    if (card.dataset.path === path) {
      const slot = card.querySelector('[data-slot="status"]');
      if (slot) {
        slot.innerHTML = renderStatus(path);
      }
      renderFilesSlot(path);
      return;
    }
  }
}

export function renderFilesSlot(path) {
  const cards = listEl.querySelectorAll('.project-card');
  for (const card of cards) {
    if (card.dataset.path === path) {
      const slot = card.querySelector('[data-slot="files"]');
      if (slot) {
        slot.innerHTML = renderFiles(path);
      }
      return;
    }
  }
}

export function renderStatusBar(busy) {
  const btn = document.getElementById('btn-refresh');
  btn.textContent = busy ? 'Обновление…' : 'Обновить';
  btn.disabled = busy;
}

export function showModal(title, body) {
  document.getElementById('modal-title').textContent = title;
  document.getElementById('modal-body').textContent = body;
  document.getElementById('modal-backdrop').classList.remove('hidden');
}

export function hideModal() {
  document.getElementById('modal-backdrop').classList.add('hidden');
}