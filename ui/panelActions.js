import * as api from './api.js';
import { state, applySnapshot, isPending, setPending } from './state.js';
import {
  panel,
  panelEl,
  refreshPanel,
  renderStatusFor,
  setPanelError,
  updatePendingUi,
} from './statusView.js';

const ACTIONS = {
  stage: 'Индексация…',
  'stage-all': 'Индексация…',
  unstage: 'Снятие с индекса…',
  discard: 'Откат…',
  commit: 'Коммит…',
  push: 'Push…',
};

/**
 * Нажали кнопку git-операции: показали крутилку, выполнили операцию и
 * отрисовали ФАКТИЧЕСКИЙ статус, пришедший в ответе команды. Промежуточных
 * «правдоподобных» цифр не рисуем — если операция упала, показываем ошибку
 * и перечитываем реальное состояние.
 */
export async function runAction(key, action) {
  const path = state.panelPath;
  if (!path || isPending(path)) {
    return;
  }
  const label = ACTIONS[key] || 'Операция…';
  setPending(path, label);
  updatePendingUi();
  setPanelError('');
  let failed = false;
  try {
    applySnapshot(await action(path));
  } catch (err) {
    failed = true;
    setPanelError(String(err));
    api.logFront(`[${key}] ${String(err)}`);
  } finally {
    setPending(path, '');
    if (failed) {
      await refreshPanel();
    } else {
      renderStatusFor(path);
    }
    updatePendingUi();
  }
}

export function onDiscard(filePath) {
  if (!window.confirm(`Откатить изменения в «${filePath}»?\nДействие необратимо.`)) {
    return;
  }
  return runAction('discard', (path) => api.discardFiles(path, [filePath]));
}

export function onStage(filePath) {
  return runAction('stage', (path) => api.stageFiles(path, [filePath]));
}

export function onStageAll() {
  const paths = (panel().unstagedFiles ?? []).map((f) => f.path);
  if (paths.length === 0) {
    return undefined;
  }
  return runAction('stage-all', (path) => api.stageFiles(path, paths));
}

export function onUnstage(filePath) {
  return runAction('unstage', (path) => api.unstageFiles(path, [filePath]));
}

export function onCommit() {
  const textarea = panelEl.querySelector('#commit-message');
  const message = textarea ? textarea.value : state.panelMessage;
  if (!message.trim()) {
    setPanelError('Введите сообщение коммита');
    return undefined;
  }
  clearTimeout(state.panelSaveTimer);
  state.panelSaveTimer = null;
  return runAction('commit', async (path) => {
    const snapshot = await api.commitChanges(path, message);
    state.panelMessage = '';
    if (textarea) {
      textarea.value = '';
    }
    return snapshot;
  });
}

export function onPush() {
  return runAction('push', (path) => api.pushChanges(path));
}

export function onGenerate() {
  const path = state.panelPath;
  if (!path) {
    return undefined;
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
    return undefined;
  }
  if (spinner) {
    spinner.classList.remove('hidden');
  }
  if (resultSlot) {
    resultSlot.classList.add('hidden');
    resultSlot.textContent = '';
  }
  setPanelError('');
  return generate(path, notes, scope, resultSlot, spinner);
}

async function generate(path, notes, scope, resultSlot, spinner) {
  try {
    const lang = state.commitLang || 'ru';
    api.logFront(
      `[generate] path=${path} lang=${lang} scope=${scope} model=${state.commitModel} notes=${notes.length} chars`,
    );
    const messages = await api.getCommitDiff(path, notes, lang, scope);
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
