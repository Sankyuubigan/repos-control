import * as api from './api.js';
import { state } from './state.js';
import { esc } from './render.js';
import { panelEl } from './statusView.js';

/** Сколько коммитов догружается за один запрос (кнопка «Показать ещё»). */
export const HISTORY_PAGE_SIZE = 20;

/** Локальная дата «ДД.ММ.ГГГГ ЧЧ:ММ» из unix-секунд коммиттера. */
function formatDateTime(unixSeconds) {
  const d = new Date(unixSeconds * 1000);
  if (Number.isNaN(d.getTime())) {
    return '';
  }
  const pad = (n) => String(n).padStart(2, '0');
  return `${pad(d.getDate())}.${pad(d.getMonth() + 1)}.${d.getFullYear()} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function commitRow(entry) {
  const head = entry.isHead
    ? '<span class="history-badge" title="Текущий коммит ветки">HEAD</span>'
    : '';
  return `<div class="history-row" title="${esc(entry.message)}">
    <span class="history-hash">${esc(entry.shortId)}</span>${head}
    <span class="history-message">${esc(entry.message)}</span>
    <span class="history-date">${esc(formatDateTime(entry.time))}</span>
  </div>`;
}

function renderCommits(commits) {
  const list = panelEl.querySelector('[data-slot="history-list"]');
  const count = panelEl.querySelector('[data-slot="history-count"]');
  if (!list) {
    return;
  }
  if (count) {
    count.textContent = commits.length > 0 ? String(commits.length) : '';
  }
  if (commits.length === 0) {
    list.innerHTML = '<div class="history-empty">В репозитории ещё нет коммитов</div>';
    return;
  }
  // Пришли все запрошенные — возможно, есть ещё. Меньше — история закончилась.
  const more =
    commits.length >= state.panelHistoryLimit
      ? '<button class="btn btn-small history-more" data-action="history-more">Показать ещё</button>'
      : '';
  list.innerHTML = `${commits.map(commitRow).join('')}${more}`;
}

/**
 * Загружает историю коммитов открытого проекта в плашку. `reset` возвращает
 * глубину к одной странице (используется при открытии панели).
 */
export async function loadCommitHistory(reset = false) {
  if (reset) {
    state.panelHistoryLimit = HISTORY_PAGE_SIZE;
  }
  const path = state.panelPath;
  const list = panelEl.querySelector('[data-slot="history-list"]');
  if (!path || !list) {
    return;
  }
  list.innerHTML = '<div class="history-empty">Загрузка истории…</div>';
  const count = panelEl.querySelector('[data-slot="history-count"]');
  if (count) {
    count.textContent = '';
  }
  try {
    const commits = await api.listCommits(path, state.panelHistoryLimit);
    if (state.panelPath !== path) {
      return; // панель уже закрыта или открыт другой проект
    }
    renderCommits(commits);
  } catch (err) {
    list.innerHTML = `<div class="history-error">История недоступна: ${esc(String(err))}</div>`;
    api.logFront(`[loadCommitHistory] ${path}: ${String(err)}`);
  }
}

export function showMoreCommits() {
  state.panelHistoryLimit += HISTORY_PAGE_SIZE;
  void loadCommitHistory();
}
