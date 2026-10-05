// The one card where the app asks what it should look like. A choice applies as
// it is picked, so the card has nothing to confirm and nothing to cancel
// (docs/techspec.md 5).
import { checkField, closeButton, modalDialog, note } from './form.js';
import { t } from './i18n.js';
import { chooseTheme, currentTheme } from './theme.js';

const themes = ['system', 'light', 'dark'];

function themeField() {
  const block = document.createElement('div');
  block.className = 'field';

  // A radio group has no single control for a `<label>` to point at, so the
  // caption is the group's own name instead.
  const caption = document.createElement('div');
  caption.className = 'group';
  caption.dataset.i18n = 'settings.theme';

  const group = document.createElement('div');
  group.className = 'radios';
  group.setAttribute('role', 'radiogroup');
  group.dataset.i18nLabel = 'settings.theme';
  for (const theme of themes) {
    const input = document.createElement('input');
    input.type = 'radio';
    input.name = 'theme';
    input.value = theme;
    input.checked = theme === currentTheme();
    input.addEventListener('change', () => chooseTheme(theme));
    group.append(checkField(`settings.theme.${theme}`, input));
  }

  block.append(caption, group, note('settings.theme.note'));
  return block;
}

export function settingsDialog() {
  const dialog = modalDialog();

  function show() {
    // Built on every look, so the card opens on the choice that is in effect now
    // rather than on the one that was current when the window was.
    const body = document.createElement('div');
    body.append(themeField());
    dialog.openModal(t('settings.title'), body, [closeButton(dialog)]);
  }

  return { dialog, show };
}
