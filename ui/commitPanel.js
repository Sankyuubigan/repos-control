import * as api from './api.js';
import * as optimistic from './optimistic.js';
import { state, setStatus } from './state.js';
import { esc, projectName, renderStatusSlot, renderProjectList } from './render.js';
import { fileIconHtml } from './fileIcons.js';

const panelEl = document.getElementById('commit-panel');

function panel() {
  const v = state.statuses[state.panelPath];
  if (!v) {
    api.logFront(
      `[panel-fallback] panelPath=${state.panelPath} hasStatus=${Object.prototype.hasOwnProperty.call(state.statuses, state.panelPath)} keys=${Object.keys(state.statuses).length} val=${JSON.stringify(v)}`,
    );
  }
  return (
    v || {
      loading: true,
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
    api.logFront(
      `[panel-fallback] panelPath=${state.panelPath} hasStatus=${Object.prototype.hasOwnProperty.call(state.statuses, state.panelPath)} keys=${Object.keys(state.statuses).length}`,
    );
    slot.innerHTML = `<div class="panel-loading">Загрузка статуса…</div>`;
    updateActionButtons(st);
    return;
  }
  if (!st.isRepo) {
    api.logFront(`[panel!repo] ${state.panelPath}: ${st.error || 'нет error'}`);
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
  setPanelError('');
  try {
    const status = await api.getProjectStatus(path);
    setStatus(path, status);
    renderPanelSections();
    renderStatusSlot(path);
  } catch (err) {
    const message = String(err);
    setStatus(path, { isRepo: false, error: message });
    renderPanelSections();
    setPanelError(message);
    renderStatusSlot(path);
    api.logFront(`[refreshPanel] ${path}: ${message}`);
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

async function runAction(label, action, applyOptimistic) {
  const path = state.panelPath;
  if (!path) {
    return;
  }
  setBusy(true);
  setPanelError('');
  try {
    if (applyOptimistic) {
      const next = applyOptimistic(panel());
      setStatus(path, next);
      renderPanelSections();
      renderStatusSlot(path);
    }
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
  await runAction(
    'discard',
    () => api.discardFiles(state.panelPath, [path]),
    (st) => optimistic.discard(st, [path]),
  );
}

async function onStage(path) {
  await runAction(
    'stage',
    () => api.stageFiles(state.panelPath, [path]),
    (st) => optimistic.stage(st, [path]),
  );
}

async function onStageAll() {
  const st = panel();
  const paths = (st.unstagedFiles ?? []).map((f) => f.path);
  if (paths.length === 0) {
    return;
  }
  await runAction(
    'stage-all',
    () => api.stageFiles(state.panelPath, paths),
    (st2) => optimistic.stage(st2, paths),
  );
}

async function onUnstage(path) {
  await runAction(
    'unstage',
    () => api.unstageFiles(state.panelPath, [path]),
    (st) => optimistic.unstage(st, [path]),
  );
}

async function onCommit() {
  const textarea = panelEl.querySelector('#commit-message');
  const message = textarea ? textarea.value : state.panelMessage;
  if (!message.trim()) {
    setPanelError('Введите сообщение коммита');
    return;
  }
  clearTimeout(state.panelSaveTimer);
  state.panelSaveTimer = null;
  await runAction(
    'commit',
    async () => {
      await api.commitChanges(state.panelPath, message);
      state.panelMessage = '';
      if (textarea) {
        textarea.value = '';
      }
    },
    (st) => optimistic.commit(st),
  );
}

async function onPush() {
  await runAction('push', () => api.pushChanges(state.panelPath), (st) => optimistic.push(st));
}

async function onPanelGenerate() {
  const path = state.panelPath;
  if (!path) {
    return;
  }
  const notesEl = panelEl.querySelector('#generate-notes');
  const scopeEl = panelEl.querySelector('input[name="generate-scope"]:checked');
  const resultSlot = panelEl.querySelector('[data-slot="generate-result"]');
  const spinner = panelEl.querySelector('[data-slot="generate-spinner"]');
  const notes = notesEl ? notesEl.value : '';
  const scope = scopeEl ? scopeEl.value : 'staged';
  state.commitScope = scope;
  if (!state.commitModel) {
    setPanelError('Модель не выбрана. Откройте «Настройки» и выберите модель для commit-сообщений.');
    return;
  }
  setBusy(true);
  setPanelError('');
  if (spinner) {
    spinner.classList.remove('hidden');
  }
  if (resultSlot) {
    resultSlot.classList.add('hidden');
    resultSlot.textContent = '';
  }
  try {
    const lang = state.commitLang || 'ru';
    api.logFront(`[generate] path=${path} lang=${lang} scope=${scope} model=${state.commitModel} notes=${notes.length} chars`);
    const messages = await api.getCommitDiff(path, notes, lang, scope);
    api.logFront(`[generate] messages count=${messages.length} system=${messages[0]?.content?.slice(0, 80)}...`);
    const message = await generateViaPlugin(state.commitModel, messages);
    api.logFront(`[generate] result=${message.slice(0, 120)}...`);
    if (resultSlot) {
      resultSlot.textContent = message;
      resultSlot.classList.remove('hidden');
    }
  } catch (err) {
    setPanelError(String(err));
    api.logFront(`[generate] ${String(err)}`);
  } finally {
    setBusy(false);
    if (spinner) {
      spinner.classList.add('hidden');
    }
  }
}

async function generateViaPlugin(modelId, messages) {
  if (modelId.startsWith('llama:')) {
    const llama = window.__TAURI__?.['llama-engine'];
    if (!llama) {
      throw new Error('Плагин llama-engine недоступен');
    }
    return llama.generateText({
      modelPath: modelId.slice('llama:'.length),
      messages,
      maxTokens: 256,
      temperature: 0.3,
    });
  }
  if (modelId.startsWith('cloud:')) {
    const cloud = window.__TAURI__?.['cloud-routers'];
    if (!cloud) {
      throw new Error('Плагин cloud-routers недоступен');
    }
    const rest = modelId.slice('cloud:'.length);
    const sep = rest.indexOf(':');
    if (sep < 0) {
      throw new Error(`Некорректный идентификатор модели: ${modelId}`);
    }
    const router = rest.slice(0, sep);
    const combo = rest.slice(sep + 1);
    return cloud.chatCompletion(router, {
      model: combo,
      messages,
      maxTokens: 256,
      temperature: 0.3,
    });
  }
  throw new Error(`Неизвестный провайдер модели: ${modelId}`);
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
        case 'stage-all':
          onStageAll();
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
    } else if (target.id === 'btn-panel-generate' && state.panelPath) {
      onPanelGenerate();
    }
  });
  panelEl.addEventListener('input', (event) => {
    if (event.target.id === 'commit-message') {
      saveMessageSoon();
    }
  });
}