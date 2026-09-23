import { SETI_DEFS, SETI_EXT, SETI_FILES } from './seti.data.js';

function baseName(path) {
  const str = String(path);
  const i = Math.max(str.lastIndexOf('/'), str.lastIndexOf('\\'));
  return str.slice(i + 1).toLowerCase();
}

export function iconForPath(path) {
  const name = baseName(path);
  if (SETI_FILES.has(name)) {
    return SETI_FILES.get(name);
  }
  const parts = name.split('.');
  for (let i = parts.length - 1; i >= 1; i--) {
    const key = parts.slice(i).join('.');
    const icon = SETI_EXT.get(key);
    if (icon) {
      return icon;
    }
  }
  return null;
}

export function fileIconHtml(path) {
  const icon = iconForPath(path);
  if (!icon) {
    return '';
  }
  const def = SETI_DEFS.get(icon);
  if (!def) {
    return '';
  }
  const style = def.length > 1 ? ` style="color:${def[1]}"` : '';
  return `<span class="file-icon"${style}>${def[0]}</span>`;
}