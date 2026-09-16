import { state } from './state.js';

const listEl = document.getElementById('project-list');
const emptyEl = document.getElementById('empty-state');

function esc(text) {
  const div = document.createElement('div');
  div.textContent = String(text);
  return div.innerHTML;
}

function badge(label, cls, title, muted) {
  const extra = muted ? ' badge-muted' : '';
  return `<span class="badge badge-${cls}${extra}" title="${esc(title)}">${esc(label)}</span>`;
}

function renderStatus(path) {
  const st = state.statuses[path];
  if (!st) {
    return badge('…', 'branch', 'загрузка');
  }
  if (!st.isRepo) {
    return `<div class="error-banner">Не git-репозиторий: ${esc(st.error || '?')}</div>`;
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
  return badges.join('');
}

function projectName(path) {
  const trimmed = String(path).replace(/[\\/]+$/, '');
  const i = Math.max(trimmed.lastIndexOf('\\'), trimmed.lastIndexOf('/'));
  return i >= 0 ? trimmed.slice(i + 1) : trimmed;
}

function projectCard(project) {
  const path = project.path;
  return `<div class="project-card" data-path="${esc(path)}">
    <div class="project-head">
      <div>
        <div class="project-name">${esc(projectName(path))}</div>
        <div class="project-path" title="${esc(path)}">${esc(path)}</div>
      </div>
      <div class="project-actions">
        <button class="btn btn-generation" data-action="generate" data-path="${esc(path)}">Сгенерировать сообщение</button>
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
      return `<div class="file-row"><span class="file-status file-status-${esc(s)}">${esc(s)}</span><span class="file-path-item">${esc(f.path)}</span></div>`;
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
  listEl.innerHTML = state.projects.map(projectCard).join('');
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