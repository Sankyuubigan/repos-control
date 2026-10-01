import * as api from './api.js';
import {
  state,
  applySnapshot,
  isPending,
  isLoading,
  pendingLabel,
  setPending,
  setLoading,
  setStatus,
} from './state.js';
import { esc, projectName, renderStatusSlot } from './render.js';
import { fileIconHtml } from './fileIcons.js';

export const panelEl = document.getElementById('commit-panel');

const EMPTY_STATUS = {
  loading: true,
  isRepo: false,
  changedFiles: [],
  stagedFiles: [],
  unstagedFiles: [],
};

export function panel() {
  return state.statuses[state.panelPath] || EMPTY_STATUS;
}

export function fileRow(file, actionHtml) {
  const s = String(file.status).toUpperCase();
  const deleted = s === 'D' ? ' is-deleted' : '';
  return `<div class="panel-file-row">
    ${fileIconHtml(file.path)}
    <span class="file-path-item${deleted}" title="${esc(file.path)}">${esc(file.path)}</span>
    <span class="file-status file-status-${esc(s)}">${esc(s)}</span>
    <span class="panel-file-actions">${actionHtml(file.path)}</span>
  </div>`;
}

function section(title, files, renderAction, headActionHtml) {
  const empty = files.length === 0 ? '<div class="panel-empty">нет изменений</div>' : '';
  const headAction = headActionHtml ? `<span class="panel-section-actions">${headActionHtml}</span>` : '';
  return `<div class="panel-section">
    <div class="panel-section-head">${esc(title)} <span class="panel-count">${files.length}</span>${headAction}</div>
    <div class="panel-files">${files.map((f) => fileRow(f, renderAction)).join('')}${empty}</div>
  </div>`;
}

function stageButton(path) {
  return `<button class="btn btn-tiny" data-action="stage" data-path="${esc(path)}" title="Добавить в индекс">+</button>`;
}

function stageAllButton(files) {
  const disabled = files.length === 0 ? ' disabled' : '';
  return `<button class="btn btn-tiny" data-action="stage-all" title="Добавить все изменения в индекс"${disabled}>+</button>`;
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
  if (st.loading) {
    slot.innerHTML = `<div class="panel-loading">Загрузка статуса…</div>`;
    updateActionButtons(st);
    return;
  }
  if (!st.isRepo) {
    slot.innerHTML = `<div class="error-banner">${esc(st.error || 'Репозиторий недоступен')}</div>`;
    updateActionButtons(st);
    return;
  }
  const staged = section('Стейдж индекс', st.stagedFiles, unstageButton);
  const unstaged = section(
    'Изменения',
    st.unstagedFiles,
    (p) => `${stageButton(p)}${discardButton(p)}`,
    stageAllButton(st.unstagedFiles),
  );
  slot.innerHTML = `${staged}${unstaged}`;
  updateActionButtons(st);
}

/**
 * Кнопки блокируются только на время идущей операции. Панель при этом
 * остаётся полностью кликабельной: прокрутка, чтение, закрытие работают.
 */
function updateActionButtons(st) {
  const path = state.panelPath;
  const busy = Boolean(path) && (isPending(path) || isLoading(path));
  panelEl.querySelectorAll('.panel-file-actions button').forEach((btn) => {
    btn.disabled = busy;
  });
  const commitBtn = panelEl.querySelector('#btn-panel-commit');
  if (commitBtn) {
    commitBtn.disabled = busy || !(st.isRepo && (st.stagedFiles?.length ?? 0) > 0);
  }
  const pushBtn = panelEl.querySelector('#btn-panel-push');
  if (pushBtn) {
    pushBtn.disabled = busy || !(st.isRepo && st.hasUpstream && (st.ahead ?? 0) > 0);
  }
  const generateBtn = panelEl.querySelector('#btn-panel-generate');
  if (generateBtn) {
    generateBtn.disabled = busy;
  }
}

/** Обновляет индикатор операции: крутилка + человеческое название действия. */
export function updatePendingUi() {
  const path = state.panelPath;
  const label = path ? pendingLabel(path) : '';
  const loading = Boolean(path) && isLoading(path);
  panelEl.classList.toggle('panel-busy', Boolean(label) || loading);
  const slot = panelEl.querySelector('[data-slot="panel-pending"]');
  if (slot) {
    slot.textContent = label;
  }
  updateActionButtons(panel());
}

/** Ветка и ahead/behind в шапке панели — обновляются после каждой операции. */
export function renderPanelMeta() {
  const slot = panelEl.querySelector('[data-slot="panel-branch"]');
  if (!slot) {
    return;
  }
  const st = panel();
  const branch = st.isRepo && st.branch ? st.branch : '';
  const up = st.isRepo && st.hasUpstream ? ` · ahead ${st.ahead} · behind ${st.behind}` : '';
  slot.textContent = `${branch}${up}`;
}

/** Применяет пришедший статус к карточке проекта и (если открыта) к панели. */
export function renderStatusFor(path) {
  renderStatusSlot(path);
  if (state.panelPath === path) {
    renderPanelMeta();
    renderPanelSections();
    updatePendingUi();
  }
}

function renderPanelHead() {
  return `<div class="panel-head">
    <button class="btn btn-small" data-action="close-panel">← Назад</button>
    <div class="panel-title">
      <div class="project-name">${esc(projectName(state.panelPath))}</div>
      <div class="panel-meta">
        <span data-slot="panel-branch"></span>
        <span class="panel-pending" data-slot="panel-pending"></span>
        <span class="panel-spinner"></span>
      </div>
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
        <span class="panel-hint">Черновик сохраняется автоматически</span>
        <button id="btn-panel-commit" class="btn btn-primary">Коммит</button>
        <button id="btn-panel-push" class="btn">Запушить</button>
      </div>
      <div class="error-banner hidden" data-slot="panel-error"></div>
    </div>
    <div class="generate-panel">
      <div class="generate-panel-head">
        <span class="generate-panel-title">Генерация сообщения</span>
        <span class="generate-spinner hidden" data-slot="generate-spinner"></span>
      </div>
      <textarea id="generate-notes" rows="2" placeholder="Заметки разработчика (необязательно)…"></textarea>
      <div class="generate-scope">
        <label><input type="radio" name="generate-scope" value="staged" ${state.commitScope === 'staged' ? 'checked' : ''}> Стейдж индекс</label>
        <label><input type="radio" name="generate-scope" value="unstaged" ${state.commitScope === 'unstaged' ? 'checked' : ''}> Остальные изменения</label>
        <label><input type="radio" name="generate-scope" value="all" ${state.commitScope === 'all' ? 'checked' : ''}> Все незакоммиченные</label>
      </div>
      <button id="btn-panel-generate" class="btn">Сгенерировать сообщение</button>
      <div class="generate-result hidden" data-slot="generate-result"></div>
    </div>
  </div>`;
}

export function renderCommitPanel() {
  panelEl.innerHTML = `${renderPanelHead()}${renderPanelBody()}`;
  renderPanelMeta();
  renderPanelSections();
  updatePendingUi();
}

export function setPanelError(message) {
  const slot = panelEl.querySelector('[data-slot="panel-error"]');
  if (slot) {
    slot.textContent = message || '';
    slot.classList.toggle('hidden', !message);
  }
}

export function showPanel() {
  document.getElementById('project-list').classList.add('hidden');
  document.getElementById('empty-state').classList.add('hidden');
  panelEl.classList.remove('hidden');
}

export function hidePanel() {
  panelEl.classList.add('hidden');
  document.getElementById('project-list').classList.remove('hidden');
}

export function saveMessageSoon() {
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

/**
 * Перечитывает статус проекта. Используется кнопкой «Обновить» и после
 * неудачной операции, когда состояние могло измениться частично.
 * Баннер ошибки здесь НЕ сбрасывается — иначе он бы стёр сообщение об ошибке,
 * только что показанное вызывающим кодом.
 */
export async function refreshPanel() {
  const path = state.panelPath;
  if (!path) {
    return;
  }
  setLoading(path, true);
  updateActionButtons(panel());
  try {
    applySnapshot(await api.getProjectStatus(path));
  } catch (err) {
    const message = String(err);
    setStatus(path, { isRepo: false, error: message });
    setPanelError(message);
    api.logFront(`[refreshPanel] ${path}: ${message}`);
  } finally {
    setLoading(path, false);
    renderStatusFor(path);
  }
}

export function setPendingForPanel(label) {
  const path = state.panelPath;
  if (!path) {
    return;
  }
  setPending(path, label);
  updatePendingUi();
}
