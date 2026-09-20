import * as api from './api.js';
import { state, setStatus } from './state.js';
import { esc, projectName, renderStatusSlot, renderProjectList } from './render.js';

const panelEl = document.getElementById('commit-panel');

function panel() {
  return (
    state.statuses[state.panelPath] || {
      isRepo: false,
      changedFiles: [],
      stagedFiles: [],
      unstagedFiles: [],
    }
  );
}

function setBusy(value) {
  panelEl.classList.toggle('panel-busy', value);
}

function fileRow(file, actionHtml) {
  const s = String(file.status).toUpperCase();
  return `<div class="panel-file-row">
    <span class="file-status file-status-${esc(s)}">${esc(s)}</span>
    <span class="file-path-item" title="${esc(file.path)}">${esc(file.path)}</span>
    <span class="panel-file-actions">${actionHtml(file.path)}</span>
  </div>`;
}

function section(title, files, renderAction) {
  const empty = files.length === 0 ? '<div class="panel-empty">нет изменений</div>' : '';
  return `<div class="panel-section">
    <div class="panel-section-head">${esc(title)} <span class="panel-count">${files.length}</span></div>
    <div class="panel-files">${files.map((f) => fileRow(f, renderAction)).join('')}${empty}</div>
  </div>`;
}

function stageButton(path) {
  return `<button class="btn btn-tiny" data-action="stage" data-path="${esc(path)}" title="Добавить в индекс">+</button>`;
}

function unstageButton(path) {
  return `<button class="btn btn-tiny" data-action="unstage" data-path="${esc(path)}" title="Снять с индекса">−</button>`;
}

function discardButton(path) {
  return `<button class="btn btn-tiny btn-danger" data-action="discard" data-path="${esc(path)}" title="Откатить изменения">↺</button>`;
}

export function renderPanelSections() {
  const st = panel();
  const slot = panelEl.querySelector('[data-slot="panel-sections"]');
  if (!slot) {
    return;
  }
  if (!st.isRepo) {
    slot.innerHTML = `<div class="error-banner">Не git-репозиторий: ${esc(st.error || '?')}</div>`;
    updateActionButtons(st);
    return;
  }
  const staged = section('Промежуточные изменения', st.stagedFiles, unstageButton);
  const unstaged = section(
    'Изменения',
    st.unstagedFiles,
    (p) => `${stageButton(p)}${discardButton(p)}`,
  );
  slot.innerHTML = `${unstaged}${staged}`;
  updateActionButtons(st);
}

function updateActionButtons(st) {
  const commitBtn = panelEl.querySelector('#btn-panel-commit');
  const pushBtn = panelEl.querySelector('#btn-panel-push');
  if (commitBtn) {
    commitBtn.disabled = !(st.isRepo && (st.stagedFiles?.length ?? 0) > 0);
  }
  if (pushBtn) {
    pushBtn.disabled = !(st.isRepo && st.hasUpstream && (st.ahead ?? 0) > 0);
  }
}

function renderPanelHead() {
  const st = panel();
  const branch = st.isRepo && st.branch ? st.branch : '';
  const up = st.isRepo && st.hasUpstream ? ` · ahead ${st.ahead} · behind ${st.behind}` : '';
  return `<div class="panel-head">
    <button class="btn btn-small" data-action="close-panel">← Назад</button>
    <div class="panel-title">
      <div class="project-name">${esc(projectName(state.panelPath))}</div>
      <div class="panel-meta">${esc(branch)}${esc(up)}<span class="panel-spinner"></span></div>
    </div>
    <button class="btn btn-primary btn-small" data-action="panel-refresh">Обновить</button>
  </div>`;
}

function renderPanelBody() {
  return `<div class="panel-body">
    <div data-slot="panel-sections"></div>
    <div class="panel-commit">
      <textarea id="commit-message" rows="4" placeholder="Сообщение коммита…">${esc(state.panelMessage)}</textarea>
      <div class="panel-actions">
        <span class="panel-hint">Хранится в .git/COMMIT_EDITMSG</span>
        <button id="btn-panel-commit" class="btn btn-primary">Коммит</button>
        <button id="btn-panel-push" class="btn">Запушить</button>
      </div>
      <div class="error-banner hidden" data-slot="panel-error"></div>
    </div>
  </div>`;
}

export function renderCommitPanel() {
  panelEl.innerHTML = `${renderPanelHead()}${renderPanelBody()}`;
  renderPanelSections();
}

function setPanelError(message) {
  const slot = panelEl.querySelector('[data-slot="panel-error"]');
  if (slot) {
    slot.textContent = message || '';
    slot.classList.toggle('hidden', !message);
  }
}

function hideList() {
  document.getElementById('project-list').classList.add('hidden');
  document.getElementById('empty-state').classList.add('hidden');
  panelEl.classList.remove('hidden');
}

export async function openCommitPanel(path) {
  state.panelPath = path;
  state.panelMessage = '';
  renderCommitPanel();
  hideList();
  const message = await api.readCommitMessage(path).catch(() => '');
  state.panelMessage = message;
  const textarea = panelEl.querySelector('#commit-message');
  if (textarea) {
    textarea.value = message;
  }
  await refreshPanel();
}

export function closeCommitPanel() {
  clearTimeout(state.panelSaveTimer);
  state.panelPath = null;
  state.panelMessage = '';
  panelEl.classList.add('hidden');
  document.getElementById('project-list').classList.remove('hidden');
  renderProjectList();
}

export async function refreshPanel() {
  const path = state.panelPath;
  if (!path) {
    return;
  }
  setBusy(true);
  try {
    const status = await api.getProjectStatus(path);
    setStatus(path, status);
    renderPanelSections();
    renderStatusSlot(path);
  } catch (err) {
    setPanelError(String(err));
    api.logFront(`[refreshPanel] ${String(err)}`);
  } finally {
    setBusy(false);
  }
}

function saveMessageSoon() {
  const textarea = panelEl.querySelector('#commit-message');
  if (!textarea) {
    return;
  }
  clearTimeout(state.panelSaveTimer);
  state.panelMessage = textarea.value;
  state.panelSaveTimer = setTimeout(async () => {
    if (!state.panelPath) {
      return;
    }
    try {
      await api.writeCommitMessage(state.panelPath, textarea.value);
    } catch (err) {
      api.logFront(`[writeCommitMessage] ${String(err)}`);
    }
  }, 400);
}

async function runAction(label, action) {
  setBusy(true);
  setPanelError('');
  try {
    await action();
    await refreshPanel();
  } catch (err) {
    setPanelError(String(err));
    api.logFront(`[${label}] ${String(err)}`);
  } finally {
    setBusy(false);
  }
}

async function onDiscard(path) {
  if (!window.confirm(`Откатить изменения в «${path}»?\nДействие необратимо.`)) {
    return;
  }
  await runAction('discard', () => api.discardFiles(state.panelPath, [path]));
}

async function onStage(path) {
  await runAction('stage', () => api.stageFiles(state.panelPath, [path]));
}

async function onUnstage(path) {
  await runAction('unstage', () => api.unstageFiles(state.panelPath, [path]));
}

async function onCommit() {
  const textarea = panelEl.querySelector('#commit-message');
  const message = textarea ? textarea.value : state.panelMessage;
  if (!message.trim()) {
    setPanelError('Введите сообщение коммита');
    return;
  }
  await runAction('commit', async () => {
    await api.commitChanges(state.panelPath, message);
    state.panelMessage = '';
    if (textarea) {
      textarea.value = '';
    }
  });
}

async function onPush() {
  await runAction('push', () => api.pushChanges(state.panelPath));
}

export function bindCommitPanelHandlers() {
  panelEl.addEventListener('click', (event) => {
    const target = event.target;
    const btn = target.closest('button[data-action]');
    if (btn && state.panelPath) {
      switch (btn.dataset.action) {
        case 'close-panel':
          closeCommitPanel();
          return;
        case 'panel-refresh':
          refreshPanel();
          return;
        case 'stage':
          onStage(btn.dataset.path);
          return;
        case 'unstage':
          onUnstage(btn.dataset.path);
          return;
        case 'discard':
          onDiscard(btn.dataset.path);
          return;
      }
    }
    if (target.id === 'btn-panel-commit' && state.panelPath) {
      onCommit();
    } else if (target.id === 'btn-panel-push' && state.panelPath) {
      onPush();
    }
  });
  panelEl.addEventListener('input', (event) => {
    if (event.target.id === 'commit-message') {
      saveMessageSoon();
    }
  });
}