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

// A card is rebuilt every time it opens, so all a reload can leave behind is the
// knowledge that this one was open when the window changed its language.
const reopenKey = 'settings.card';

/// Read once by `main.js` after a window is built: a theme applies in place and
/// the card stays open, and a language has to rebuild the screen around it, which
/// would otherwise answer a click on a radio with a closed card.
export function takeSettingsCard() {
  const wanted = sessionStorage.getItem(reopenKey) === '1';
  sessionStorage.removeItem(reopenKey);
  return wanted;
}

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
    (value) => {
      sessionStorage.setItem(reopenKey, '1');
      return chooseLanguage(value);
    },
    'settings.language.note'
  );
}

export function settingsDialog() {
  const dialog = modalDialog();

  function show() {
    // Built on every look, so the card opens on the choice that is in effect now
    // rather than on the one that was current when the window was.
    const body = document.createElement('div');
    body.className = 'settings';
    // The card's own caption says which card this is, the way the store's does:
    // the dialog's title is only read by a screen reader, and two questions
    // without a heading over them read as a piece of the form behind.
    const title = document.createElement('p');
    title.className = 'group';
    title.dataset.i18n = 'settings.title';
    body.append(
      title,
      themeField(),
      // One language is not a choice, so the row only appears once a second
      // dictionary is registered in i18n.js.
      ...(languages.length > 1 ? [languageField()] : [])
    );
    dialog.openModal(t('settings.title'), body, [closeButton(dialog)]);
  }

  return { dialog, show };
}
