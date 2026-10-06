// The strip that replaced the native caption (docs/techspec.md 5). The window is
// undecorated, so this is the part that drags it and the part whose buttons
// minimize, maximize and close it.
import { t } from './i18n.js';

// The glyphs come from the icon font Windows itself uses for its caption
// buttons, so the strip reads as another window titlebar rather than as a web
// page wearing one. They are private-use codepoints, so they are written as
// escapes rather than as characters that no editor shows.
const glyph = {
  settings: '\uE713',
  minimize: '\uE921',
  maximize: '\uE922',
  restore: '\uE923',
  about: '\uE946',
  close: '\uE8BB',
};

const appWindow = window.__TAURI__.window.getCurrentWindow();

function button(kind, action) {
  const element = document.createElement('button');
  element.type = 'button';
  element.className = `caption-${kind}`;
  element.textContent = glyph[kind];
  // A button of one glyph has nothing to read, so its name is also its tooltip.
  element.setAttribute('aria-label', t(`titlebar.${kind}`));
  element.title = t(`titlebar.${kind}`);
  element.addEventListener('click', action);
  return element;
}

/// The maximize button shows a different glyph and name while the window is
/// maximized. Snapping with Win+arrow or dragging to an edge changes that state
/// without a click of ours, so it is read back from the window each time.
async function showMaximized() {
  const element = document.querySelector('.caption-maximize');
  const maximized = await appWindow.isMaximized();
  const kind = maximized ? 'restore' : 'maximize';
  element.textContent = glyph[kind];
  element.setAttribute('aria-label', t(`titlebar.${kind}`));
  element.title = t(`titlebar.${kind}`);
}

export function titlebar({ onSettings, onAbout }) {
  const root = document.getElementById('titlebar');

  const brand = document.createElement('h1');
  brand.className = 'brand';
  brand.dataset.i18n = 'app.title';
  // The caption of a window is part of the bar you drag, so the text drags too.
  brand.setAttribute('data-tauri-drag-region', '');

  const drag = document.createElement('div');
  drag.className = 'drag';
  drag.setAttribute('data-tauri-drag-region', '');

  root.append(
    brand,
    drag,
    // The window's own buttons stay in the corner, so the two cards the app
    // offers sit to their left rather than beside the brand. The one a person
    // reaches for keeps the place nearest the corner and the one they only need
    // once moves out of its way, which also reads left to right as `what this
    // is` then `how it behaves`.
    button('about', onAbout),
    button('settings', onSettings),
    button('minimize', () => appWindow.minimize()),
    button('maximize', () => appWindow.toggleMaximize().then(showMaximized)),
    button('close', () => appWindow.close())
  );

  showMaximized();
  appWindow.listen('tauri://resize', showMaximized).catch(() => {});
}
