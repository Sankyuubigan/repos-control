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
import { currentScope, generateNotes, MAX_GEN_TOKENS, refreshEstimate } from './generatePanel.js';
import { loadCommitHistory } from './historyPanel.js';

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
      if (key === 'commit') {
        void loadCommitHistory();
      }
    }
    updatePendingUi();
    void refreshEstimate();
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
  const resultSlot = panelEl.querySelector('[data-slot="generate-result"]');
  const spinner = panelEl.querySelector('[data-slot="generate-spinner"]');
  state.commitScope = currentScope();
  if (!state.commitModel) {
    setPanelError('Модель не выбрана — выберите её в плашке генерации.');
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
  return generate(path, resultSlot, spinner);
}

async function generate(path, resultSlot, spinner) {
  try {
    const lang = state.commitLang || 'ru';
    api.logFront(
      `[generate] path=${path} lang=${lang} scope=${state.commitScope} model=${state.commitModel} notes=${generateNotes().length} chars`,
    );
    const messages = await api.getCommitDiff(path, generateNotes(), lang, state.commitScope);
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
      maxTokens: MAX_GEN_TOKENS,
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
      maxTokens: MAX_GEN_TOKENS,
      temperature: 0.3,
    });
  }
  throw new Error(`Неизвестный провайдер модели: ${modelId}`);
}
