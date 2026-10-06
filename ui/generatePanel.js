import * as api from './api.js';
import { state } from './state.js';
import { panelEl } from './statusView.js';

const ROUTERS = ['9router', 'extremerouter', 'omniroute'];

/**
 * Лимит генерации — сколько токенов LLM имеет право вернуть в commit-сообщение.
 * Единственное определение в проекте: оно же уходит в `generateText`/`chatCompletion`
 * и в `estimatePromptMemory`, где плагин считает по нему VRAM.
 */
export const MAX_GEN_TOKENS = 256;

/**
 * Резерв на служебные токены чат-шаблона. Источник: `estimate_context`
 * в `tauri-plugin-llama-engine/src/generate.rs`.
 */
const RESERVE_TOKENS = 512;

let estimateTimer;
let catalogCache;
let modelMaxCtxCache = new Map();

export function generateNotes() {
  const el = panelEl.querySelector('#generate-notes');
  return el ? el.value : '';
}

export function currentScope() {
  const checked = panelEl.querySelector('input[name="generate-scope"]:checked');
  return checked ? checked.value : 'staged';
}

function modelSelect() {
  return panelEl.querySelector('#commit-model-select');
}

function langSelect() {
  return panelEl.querySelector('#commit-lang-select');
}

function setError(message) {
  const el = panelEl.querySelector('#commit-model-error');
  if (!el) {
    return;
  }
  el.textContent = message || '';
  el.classList.toggle('hidden', !message);
}

function updateHint() {
  const el = panelEl.querySelector('#commit-model-hint');
  if (!el) {
    return;
  }
  const model = state.commitModel;
  if (!model) {
    el.textContent = 'Модель не выбрана — установите движок или роутер в «Настройках»';
  } else if (model.startsWith('llama:')) {
    el.textContent = 'Локально через llama-server';
  } else if (model.startsWith('cloud:')) {
    el.textContent = 'Через облачный роутер';
  } else {
    el.textContent = '';
  }
}

export async function loadCommitSettings() {
  const select = modelSelect();
  if (!select) {
    return;
  }
  setError('');
  const [storedModel, storedLang] = await Promise.all([
    api.getCommitModel().catch((err) => {
      api.logFront(`[generate] getCommitModel: ${String(err)}`);
      return '';
    }),
    api.getCommitLang().catch((err) => {
      api.logFront(`[generate] getCommitLang: ${String(err)}`);
      return 'ru';
    }),
  ]);
  state.commitLang = storedLang || 'ru';
  if (langSelect()) {
    langSelect().value = state.commitLang;
  }
  await refreshModelOptions(storedModel);
}

export async function refreshModelOptions(preferred) {
  const select = modelSelect();
  if (!select) {
    return;
  }
  const options = await collectModelOptions();
  select.innerHTML = '';
  if (options.length === 0) {
    const empty = document.createElement('option');
    empty.value = '';
    empty.textContent = 'Нет моделей — установите движок или роутер в «Настройках»';
    select.appendChild(empty);
    select.disabled = true;
    updateHint();
    return;
  }
  select.disabled = false;
  for (const opt of options) {
    const el = document.createElement('option');
    el.value = opt.id;
    el.textContent = opt.label;
    select.appendChild(el);
  }
  const wanted = preferred ?? state.commitModel;
  const exists = options.some((o) => o.id === wanted);
  select.value = exists ? wanted : options[0].id;
  state.commitModel = select.value;
  updateHint();
}

async function collectModelOptions() {
  const options = [];
  const llama = window.__TAURI__?.['llama-engine'];
  if (llama) {
    try {
      const cfg = await llama.getEngineConfig();
      for (const model of cfg.models || []) {
        options.push({ id: `llama:${model}`, label: `Локальная: ${fileName(model)}` });
      }
    } catch (err) {
      api.logFront(`[generate] getEngineConfig: ${String(err)}`);
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
        api.logFront(`[generate] cloud ${router}: ${String(err)}`);
      }
    }
  }
  return options;
}

export async function onModelChange() {
  const select = modelSelect();
  if (!select) {
    return;
  }
  state.commitModel = select.value;
  updateHint();
  try {
    await api.setCommitModel(state.commitModel);
  } catch (err) {
    setError(`Не удалось сохранить выбор: ${String(err)}`);
    api.logFront(`[generate] setCommitModel: ${String(err)}`);
  }
  await refreshEstimate();
}

export async function onLangChange() {
  const select = langSelect();
  if (!select) {
    return;
  }
  state.commitLang = select.value;
  try {
    await api.setCommitLang(state.commitLang);
  } catch (err) {
    api.logFront(`[generate] setCommitLang: ${String(err)}`);
  }
  await refreshEstimate();
}

export function onScopeChange() {
  state.commitScope = currentScope();
  refreshEstimate();
}

export function onNotesChange() {
  clearTimeout(estimateTimer);
  estimateTimer = setTimeout(() => void refreshEstimate(), 400);
}

export async function refreshEstimate() {
  const slot = panelEl.querySelector('[data-slot="generate-tokens-info"]');
  const path = state.panelPath;
  if (!slot || !path) {
    return;
  }
  slot.textContent = 'Считаю объём промпта…';
  slot.classList.remove('is-overflow');
  slot.classList.add('is-loading');
  try {
    const estimate = await api.estimateCommitPrompt(
      path,
      generateNotes(),
      state.commitLang,
      currentScope(),
    );
    const modelMaxCtx = await resolveModelMaxCtx();
    const tokenInfo = await countPromptTokens(estimate.prompt);
    const files = `${estimate.files} ${plural(estimate.files, 'файл', 'файла', 'файлов')}`;
    const tokens = tokenInfo.exact
      ? `${tokenInfo.tokens} ${plural(tokenInfo.tokens, 'токен', 'токена', 'токенов')}`
      : `~${tokenInfo.tokens} ${plural(tokenInfo.tokens, 'токен', 'токена', 'токенов')}`;
    const memory = await formatMemory(tokenInfo.tokens, modelMaxCtx);
    if (modelMaxCtx > 0) {
      const fits = tokenInfo.tokens + MAX_GEN_TOKENS + RESERVE_TOKENS <= modelMaxCtx;
      const budget = Math.max(modelMaxCtx - MAX_GEN_TOKENS - RESERVE_TOKENS, 0);
      slot.textContent = `${files} · ${tokens} из ${budget} · ${memory}`;
      slot.classList.toggle('is-overflow', !fits);
      slot.title = fits
        ? ''
        : `Промпт не влезет в окно движка (${modelMaxCtx} токенов) — генерация вернёт ошибку.`;
    } else {
      slot.textContent = `${files} · ${tokens} · ${memory} · контекст модели не прочитан`;
      slot.classList.remove('is-overflow');
      slot.title = 'Не удалось прочитать context_length из GGUF — бюджет окна неизвестен.';
    }
  } catch (err) {
    slot.textContent = String(err);
    slot.classList.add('is-overflow');
    api.logFront(`[generate] refreshEstimate: ${String(err)}`);
  } finally {
    slot.classList.remove('is-loading');
  }
}

/**
 * Максимальный контекст модели из GGUF (`<arch>.context_length`).
 * Кэшируется по пути модели — GGUF читается один раз.
 * 0 = прочитать не удалось (старый бинарник, битый файл, нет ключа в GGUF).
 * Хардкодных значений нет: выдуманный потолок хуже честного нуля.
 */
async function resolveModelMaxCtx() {
  const model = state.commitModel;
  if (!model.startsWith('llama:')) {
    return 0;
  }
  const modelPath = model.slice('llama:'.length);
  if (modelMaxCtxCache.has(modelPath)) {
    return modelMaxCtxCache.get(modelPath);
  }
  const llama = window.__TAURI__?.['llama-engine'];
  if (!llama?.getModelContextSize) {
    api.logFront('[generate] getModelContextSize: команда недоступна (нужна пересборка плагина)');
    return 0;
  }
  try {
    const ctx = await llama.getModelContextSize(modelPath);
    const value = ctx > 0 ? ctx : 0;
    if (value === 0) {
      api.logFront(`[generate] getModelContextSize: context_length не найден в ${modelPath}`);
    }
    modelMaxCtxCache.set(modelPath, value);
    return value;
  } catch (err) {
    api.logFront(`[generate] getModelContextSize: ${String(err)}`);
    return 0;
  }
}

/**
 * Точный подсчёт токенов через плагин. Если токенизатор недоступен (оффлайн,
 * CDN), плагин сам откатится на `chars/3` и вернёт `exact: false`.
 */
async function countPromptTokens(promptText) {
  const llama = window.__TAURI__?.['llama-engine'];
  if (!llama?.countTokens) {
    return fallbackTokens(promptText);
  }
  const tokenizerId = await resolveTokenizerId();
  if (!tokenizerId) {
    return fallbackTokens(promptText);
  }
  try {
    return await llama.countTokens(promptText, tokenizerId);
  } catch (err) {
    api.logFront(`[generate] countTokens: ${String(err)}`);
    return fallbackTokens(promptText);
  }
}

/** Фоллбэк `chars/3` — та же формула, по которой движок считает `--ctx-size`. */
function fallbackTokens(promptText) {
  return { tokens: Math.ceil(promptText.length / 3), exact: false };
}

/** Резолв `tokenizer_id` из каталога моделей плагина. */
async function resolveTokenizerId() {
  const model = state.commitModel;
  if (!model.startsWith('llama:')) {
    return '';
  }
  const modelPath = model.slice('llama:'.length);
  if (!catalogCache) {
    const llama = window.__TAURI__?.['llama-engine'];
    if (!llama?.getModelsCatalog) {
      return '';
    }
    try {
      catalogCache = await llama.getModelsCatalog();
    } catch (err) {
      api.logFront(`[generate] getModelsCatalog: ${String(err)}`);
      catalogCache = [];
    }
  }
  const entry = catalogCache.find(
    (m) => modelPath.includes(m.name) || m.name === modelPath,
  );
  return entry?.tokenizer_id || '';
}

/**
 * Требуемая память под этот промпт. Считает плагин: модель + KV-кэш на
 * эффективный контекст, плюс фактическая занятость VRAM через NVML.
 * Облачным роутерам память GPU не нужна — там только токены.
 */
async function formatMemory(promptTokens, modelMaxCtx) {
  const model = state.commitModel;
  if (!model.startsWith('llama:')) {
    return model ? 'VRAM: не применимо (облако)' : '';
  }
  if (!(modelMaxCtx > 0)) {
    return 'VRAM: нужен context_length из GGUF';
  }
  const llama = window.__TAURI__?.['llama-engine'];
  if (!llama?.estimatePromptMemory) {
    return '';
  }
  try {
    const info = await llama.estimatePromptMemory(
      model.slice('llama:'.length),
      modelMaxCtx,
      false,
      false,
      promptTokens,
      MAX_GEN_TOKENS,
    );
    const need = Math.round(info.need_mb);
    if (info.vram_total_mb > 0) {
      return `VRAM: ~${need} МБ из ${Math.round(info.vram_total_mb)} МБ (занято ${Math.round(info.vram_used_mb)} МБ)`;
    }
    return `VRAM: ~${need} МБ`;
  } catch (err) {
    api.logFront(`[generate] estimatePromptMemory: ${String(err)}`);
    return 'VRAM: оценка недоступна';
  }
}

function plural(n, one, few, many) {
  const mod10 = n % 10;
  const mod100 = n % 100;
  if (mod10 === 1 && mod100 !== 11) {
    return one;
  }
  if (mod10 >= 2 && mod10 <= 4 && (mod100 < 10 || mod100 >= 20)) {
    return few;
  }
  return many;
}

function fileName(path) {
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || path;
}

function routerLabel(router) {
  const labels = {
    '9router': '9Router',
    'extremerouter': 'ExtremeRouter',
    'omniroute': 'OmniRoute',
  };
  return labels[router] || router;
}