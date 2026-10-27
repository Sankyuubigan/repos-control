/**
 * Кастомное контекстное меню: один элемент на странице, открывается в точке
 * курсора, закрывается по клику мимо, повторному ПКМ, Escape, скроллу и
 * потере фокуса окна. Пункты передаются вызывающим кодом — модуль ничего
 * не знает про git.
 */

let menuEl = null;
let bound = false;

function bindGlobalListeners() {
  if (bound) {
    return;
  }
  bound = true;
  document.addEventListener('click', (event) => {
    if (menuEl && !menuEl.contains(event.target)) {
      closeContextMenu();
    }
  }, true);
  document.addEventListener('contextmenu', (event) => {
    if (menuEl && !menuEl.contains(event.target)) {
      closeContextMenu();
    }
  }, true);
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') {
      closeContextMenu();
    }
  }, true);
  window.addEventListener('blur', closeContextMenu);
  window.addEventListener('resize', closeContextMenu);
  // capture=true: ловим скролл любого контейнера под меню, не только window.
  window.addEventListener('scroll', closeContextMenu, true);
}

function clamp(value, min, max) {
  return Math.max(min, Math.min(value, max));
}

/**
 * Открывает меню в точке (x, y). `items` — список `{ label, onSelect }`.
 */
export function openContextMenu(x, y, items) {
  closeContextMenu();
  bindGlobalListeners();
  const menu = document.createElement('div');
  menu.className = 'ctx-menu';
  for (const item of items) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'ctx-item';
    btn.textContent = item.label;
    btn.addEventListener('click', () => {
      closeContextMenu();
      item.onSelect();
    });
    menu.appendChild(btn);
  }
  document.body.appendChild(menu);
  const rect = menu.getBoundingClientRect();
  menu.style.left = `${clamp(x, 0, window.innerWidth - rect.width)}px`;
  menu.style.top = `${clamp(y, 0, window.innerHeight - rect.height)}px`;
  menuEl = menu;
}

export function closeContextMenu() {
  menuEl?.remove();
  menuEl = null;
}
