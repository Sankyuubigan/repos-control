import * as api from './api.js';
import { state } from './state.js';
import {
  hidePanel,
  panelEl,
  refreshPanel,
  renderCommitPanel,
  renderPanelSections,
  renderStatusFor,
  saveMessageSoon,
  setPanelError,
  setPendingForPanel,
  showPanel,
} from './statusView.js';
import {
  onAddToGitignore,
  onCommit,
  onDiscard,
  onGenerate,
  onOpenGitignore,
  onPush,
  onStage,
  onStageAll,
  onUnstage,
} from './panelActions.js';
import {
  loadCommitSettings,
  onLangChange,
  onModelChange,
  onNotesChange,
  onScopeChange,
  refreshEstimate,
  refreshModelOptions,
} from './generatePanel.js';
import { loadCommitHistory, showMoreCommits } from './historyPanel.js';
import { openContextMenu } from './contextMenu.js';

export { refreshPanel, renderPanelSections, renderStatusFor };

export async function openCommitPanel(path) {
  state.panelPath = path;
  state.panelMessage = '';
  renderCommitPanel();
  showPanel();
  const message = await api.readCommitMessage(path).catch((err) => {
    api.logFront(`[readCommitMessage] ${String(err)}`);
    return '';
  });
  state.panelMessage = message;
  const textarea = panelEl.querySelector('#commit-message');
  if (textarea) {
    textarea.value = message;
  }
  try {
    await loadCommitSettings();
  } catch (err) {
    api.logFront(`[loadCommitSettings] ${String(err)}`);
  }
  await refreshPanel();
  void refreshEstimate();
  void loadCommitHistory(true);
}

export function closeCommitPanel() {
  clearTimeout(state.panelSaveTimer);
  setPendingForPanel('');
  state.panelPath = null;
  state.panelMessage = '';
  hidePanel();
}

function onPanelClick(event) {
  const btn = event.target.closest('button[data-action]');
  if (btn && state.panelPath) {
    switch (btn.dataset.action) {
      case 'close-panel':
        closeCommitPanel();
        return;
      case 'panel-refresh':
        setPanelError('');
        refreshPanel();
        void loadCommitHistory();
        return;
      case 'history-more':
        showMoreCommits();
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
      case 'open-gitignore':
        onOpenGitignore();
        return;
      case 'commit-backup-stamp':
        insertBackupStamp();
        return;
    }
  }
  if (!state.panelPath) {
    return;
  }
  if (event.target.id === 'btn-panel-commit') {
    onCommit();
  } else if (event.target.id === 'btn-panel-push') {
    onPush();
  } else if (event.target.id === 'btn-panel-generate') {
    onGenerate();
  } else if (event.target.id === 'btn-refresh-models') {
    void refreshModelOptions();
  }
}

/** Локальное время в формате «ГГГГ-ММ-ДД ЧЧ:ММ:СС». */
function formatLocalStamp() {
  const d = new Date();
  const pad = (n) => String(n).padStart(2, '0');
  return (
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}` +
    ` ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
  );
}

/** Добавляет в поле сообщения строку «backup: ГГГГ-ММ-ДД ЧЧ:ММ:СС». */
function insertBackupStamp() {
  const textarea = panelEl.querySelector('#commit-message');
  if (!textarea) {
    return;
  }
  const current = textarea.value.replace(/\s+$/, '');
  const stamp = `backup: ${formatLocalStamp()}`;
  textarea.value = current ? `${current}\n${stamp}` : stamp;
  textarea.focus();
  textarea.setSelectionRange(textarea.value.length, textarea.value.length);
  saveMessageSoon();
}

/**
 * ПКМ на строке файла (стейдж или изменения) — кастомное меню вместо
 * системного. Путь берём из data-path строки, который проставляет fileRow.
 */
function onPanelContextMenu(event) {
  const row = event.target.closest('.panel-file-row');
  if (!row || !state.panelPath) {
    return;
  }
  const filePath = row.dataset.path;
  if (!filePath) {
    return;
  }
  event.preventDefault();
  openContextMenu(event.clientX, event.clientY, [
    { label: 'Добавить в .gitignore', onSelect: () => onAddToGitignore(filePath) },
  ]);
}

function onPanelChange(event) {
  const target = event.target;
  if (target.id === 'commit-model-select') {
    void onModelChange();
  } else if (target.id === 'commit-lang-select') {
    void onLangChange();
  } else if (target.name === 'generate-scope') {
    onScopeChange();
  }
}

export function bindCommitPanelHandlers() {
  panelEl.addEventListener('click', onPanelClick);
  panelEl.addEventListener('contextmenu', onPanelContextMenu);
  panelEl.addEventListener('change', onPanelChange);
  panelEl.addEventListener('input', (event) => {
    if (event.target.id === 'commit-message') {
      saveMessageSoon();
    } else if (event.target.id === 'generate-notes') {
      onNotesChange();
    }
  });
}
