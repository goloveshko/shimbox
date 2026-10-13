// The one panel where the app asks what it should look like. It opens along the
// edge of the window rather than over it because a choice here applies as it is
// picked, and the thing it applies to is the window - which a centred card dims
// at the very moment it is being judged (docs/techspec.md 5).
import { dropdown, field, note, segmented, sideSheet } from './form.js';
import { chooseLanguage, currentLanguage, languages } from './i18n.js';
import { chooseTheme, currentTheme } from './theme.js';

// Segoe's own marks for the three answers, written as escapes because a
// private-use codepoint is not a character an editor shows.
const mark = {
  system: '\uE7F4',
  light: '\uE706',
  dark: '\uE708',
};

const themes = [
  { value: 'system', key: 'settings.theme.system', glyph: mark.system },
  { value: 'light', key: 'settings.theme.light', glyph: mark.light },
  { value: 'dark', key: 'settings.theme.dark', glyph: mark.dark },
];

const languagesToPick = [
  { value: 'system', key: 'settings.language.system' },
  ...languages.map(({ code, name }) => ({ value: code, name })),
];

/// A caption over the control that answers it, for the group - a radio set -
/// that has no single control a `<label>` could point at.
function group(captionKey, control) {
  const block = document.createElement('div');
  block.className = 'field';
  const caption = document.createElement('p');
  caption.className = 'group';
  caption.dataset.i18n = captionKey;
  block.append(caption, control);
  return block;
}

export function settingsSheet() {
  const sheet = sideSheet();

  function show() {
    // Built on every look, so the panel opens on the choice that is in effect now
    // rather than on the one that was current when the window was.
    const body = document.createElement('div');
    body.className = 'settings';
    const language = dropdown(languagesToPick, currentLanguage, chooseLanguage);
    body.append(
      // Both fields apply on the click, so the sentence they each used to carry
      // is said once for the panel instead of twice against it.
      note('settings.applies'),
      group(
        'settings.theme',
        segmented('theme', 'settings.theme', themes, currentTheme, chooseTheme)
      ),
      // One language is not a choice, so the row only appears once a second
      // dictionary is registered in i18n.js.
      ...(languages.length > 1 ? [field('settings.language', language, language)] : [])
    );
    sheet.openSheet('settings.title', body);
  }

  return { sheet, show };
}
