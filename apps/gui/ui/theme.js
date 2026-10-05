// The theme is a decision rather than a stylesheet branch: this works out once
// whether the window is light or dark, writes the answer onto the document, and
// CSS has a single set of dark tokens to match. `system` is the only choice that
// keeps listening to Windows.
import { invoke } from './form.js';

const appWindow = window.__TAURI__.window.getCurrentWindow();
const systemPrefersDark = window.matchMedia('(prefers-color-scheme: dark)');

let choice = 'system';

function paint() {
  const dark = choice === 'dark' || (choice === 'system' && systemPrefersDark.matches);
  document.documentElement.dataset.theme = dark ? 'dark' : 'light';
  // Mica, the scrollbars and a native dropdown are painted by Windows rather than
  // by this page, so it is told which of the two it is drawing. `null` hands the
  // decision back to it.
  appWindow.setTheme(choice === 'system' ? null : choice).catch(() => {});
}

// A `system` choice can be changed under us from the personalisation panel, and
// an override has nothing left to react to.
systemPrefersDark.addEventListener('change', () => {
  if (choice === 'system') {
    paint();
  }
});

/// The stored choice, applied. Awaited before the shell is built so the window
/// does not open in one theme and settle into the other.
export async function loadTheme() {
  try {
    choice = (await invoke('get_theme', {})) ?? 'system';
  } catch {
    // A setting that cannot be read is a setting that was never made.
  }
  paint();
}

/// Applies at once and stores it: a theme is judged by looking at it, so there is
/// nothing for an Apply button to wait for.
export function chooseTheme(next) {
  choice = next;
  paint();
  invoke('set_theme', { theme: next }).catch(() => {});
}

export const currentTheme = () => choice;
