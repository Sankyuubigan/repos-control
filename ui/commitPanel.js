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
  onCommit,
  onDiscard,
  onGenerate,
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
  panelEl.addEventListener('change', onPanelChange);
  panelEl.addEventListener('input', (event) => {
    if (event.target.id === 'commit-message') {
      saveMessageSoon();
    } else if (event.target.id === 'generate-notes') {
      onNotesChange();
    }
  });
}
