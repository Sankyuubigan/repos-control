import * as api from './api.js';
import { state } from './state.js';

const ROUTERS = ['9router', 'extremerouter', 'omniroute'];

let selectEl;
let hintEl;
let errorEl;
let langSelectEl;

export function initSettings() {
  selectEl = document.getElementById('commit-model-select');
  hintEl = document.getElementById('commit-model-hint');
  errorEl = document.getElementById('commit-model-error');
  langSelectEl = document.getElementById('commit-lang-select');
  selectEl.addEventListener('change', onModelChange);
  langSelectEl.addEventListener('change', onLangChange);
  document.getElementById('btn-refresh-models').addEventListener('click', () => void refreshSettings());
  void refreshSettings();
}

export async function refreshSettings() {
  if (!selectEl) {
    return;
  }
  setError('');
  const options = await collectModelOptions();
  renderOptions(options);
  await loadLang();
}

async function loadLang() {
  try {
    const lang = await api.getCommitLang();
    state.commitLang = lang || 'ru';
    langSelectEl.value = state.commitLang;
  } catch (err) {
    api.logFront(`[settings] getCommitLang: ${String(err)}`);
  }
}

async function onLangChange() {
  state.commitLang = langSelectEl.value;
  try {
    await api.setCommitLang(state.commitLang);
  } catch (err) {
    api.logFront(`[settings] setCommitLang: ${String(err)}`);
  }
}

async function collectModelOptions() {
  const options = [];
  const llama = window.__TAURI__?.['llama-engine'];
  if (llama) {
    try {
      const cfg = await llama.getEngineConfig();
      for (const path of cfg.models || []) {
        options.push({
          id: `llama:${path}`,
          label: `Локальная: ${fileName(path)}`,
        });
      }
    } catch (err) {
      api.logFront(`[settings] llama.getEngineConfig: ${String(err)}`);
    }
  }
  const cloud = window.__TAURI__?.['cloud-routers'];
  if (cloud) {
    for (const router of ROUTERS) {
      try {
        const status = await cloud.getStatus(router);
        if (!status.installed) {
          continue;
        }
        const combos = await cloud.getCombos(router);
        for (const combo of combos) {
          options.push({
            id: `cloud:${router}:${combo.name}`,
            label: `${routerLabel(router)}: ${combo.name}`,
          });
        }
      } catch (err) {
        api.logFront(`[settings] cloud ${router}: ${String(err)}`);
      }
    }
  }
  return options;
}

function renderOptions(options) {
  const previous = state.commitModel;
  selectEl.innerHTML = '';
  if (options.length === 0) {
    const empty = document.createElement('option');
    empty.value = '';
    empty.textContent = 'Нет доступных моделей — установите движок или роутер ниже';
    selectEl.appendChild(empty);
    selectEl.disabled = true;
    hintEl.textContent = '';
    return;
  }
  selectEl.disabled = false;
  for (const opt of options) {
    const el = document.createElement('option');
    el.value = opt.id;
    el.textContent = opt.label;
    selectEl.appendChild(el);
  }
  const stillExists = options.some((o) => o.id === previous);
  selectEl.value = stillExists ? previous : options[0].id;
  state.commitModel = selectEl.value;
  updateHint();
}

async function onModelChange() {
  state.commitModel = selectEl.value;
  updateHint();
  try {
    await api.setCommitModel(state.commitModel);
  } catch (err) {
    setError(`Не удалось сохранить выбор: ${String(err)}`);
    api.logFront(`[settings] setCommitModel: ${String(err)}`);
  }
}

function updateHint() {
  const model = state.commitModel;
  if (!model) {
    hintEl.textContent = '';
    return;
  }
  if (model.startsWith('llama:')) {
    hintEl.textContent = 'Генерация локально через llama-server';
  } else if (model.startsWith('cloud:')) {
    hintEl.textContent = 'Генерация через облачный роутер';
  } else {
    hintEl.textContent = '';
  }
}

function setError(message) {
  if (!errorEl) {
    return;
  }
  errorEl.textContent = message || '';
  errorEl.classList.toggle('hidden', !message);
}

function fileName(path) {
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || path;
}

function routerLabel(router) {
  if (router === '9router') {
    return '9Router';
  }
  if (router === 'extremerouter') {
    return 'ExtremeRouter';
  }
  if (router === 'omniroute') {
    return 'OmniRoute';
  }
  return router;
}
