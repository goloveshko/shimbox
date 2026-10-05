// The one card where the app asks what it should look like. A choice applies as
// it is picked, so the card has nothing to confirm and nothing to cancel
// (docs/techspec.md 5).
import { checkField, closeButton, modalDialog, note } from './form.js';
import { chooseLanguage, currentLanguage, languages, t } from './i18n.js';
import { chooseTheme, currentTheme } from './theme.js';

const themes = [
  { value: 'system', key: 'settings.theme.system' },
  { value: 'light', key: 'settings.theme.light' },
  { value: 'dark', key: 'settings.theme.dark' },
];

/// A caption, a row of radios and the hint under them. `options` carry either a
/// key to translate or a name to show as it is, which is how a language can be
/// recognized in an app that does not speak it yet.
function choiceField(name, captionKey, options, picked, onPick, noteKey) {
  const block = document.createElement('div');
  block.className = 'field';

  // A radio group has no single control for a `<label>` to point at, so the
  // caption is the group's own name instead.
  const caption = document.createElement('div');
  caption.className = 'group';
  caption.dataset.i18n = captionKey;

  const group = document.createElement('div');
  group.className = 'radios';
  group.setAttribute('role', 'radiogroup');
  group.dataset.i18nLabel = captionKey;
  for (const option of options) {
    const input = document.createElement('input');
    input.type = 'radio';
    input.name = name;
    input.value = option.value;
    input.checked = option.value === picked();
    input.addEventListener('change', () => onPick(option.value));
    group.append(checkField(option.key, input, option.name));
  }

  block.append(caption, group, note(noteKey));
  return block;
}

function themeField() {
  return choiceField(
    'theme',
    'settings.theme',
    themes,
    currentTheme,
    chooseTheme,
    'settings.theme.note'
  );
}

function languageField() {
  return choiceField(
    'language',
    'settings.language',
    [
      { value: 'system', key: 'settings.language.system' },
      ...languages.map(({ code, name }) => ({ value: code, name })),
    ],
    currentLanguage,
    chooseLanguage,
    'settings.language.note'
  );
}

export function settingsDialog() {
  const dialog = modalDialog();

  function show() {
    // Built on every look, so the card opens on the choice that is in effect now
    // rather than on the one that was current when the window was.
    const body = document.createElement('div');
    body.append(
      themeField(),
      // One language is not a choice, so the row only appears once a second
      // dictionary is registered in i18n.js.
      ...(languages.length > 1 ? [languageField()] : [])
    );
    dialog.openModal(t('settings.title'), body, [closeButton(dialog)]);
  }

  return { dialog, show };
}
