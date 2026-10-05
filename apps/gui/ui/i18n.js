// Resolves the UI language. Adding a language means dropping a file into
// locales/, registering its dictionary and its own name here; nothing else looks
// at the list, and the choice in Settings only appears once there is more than
// one language registered to choose between.
import en from './locales/en.js';
import ru from './locales/ru.js';

/// What each language calls itself, in that language: someone looking for
/// Russian has to recognize it while the app is still speaking English.
export const languages = [
  { code: 'en', name: 'English' },
  { code: 'ru', name: 'Русский' },
];

const dictionaries = { en, ru };

/// The stored answer that means "whatever Windows is set to".
const followWindows = 'system';

let choice = followWindows;
let dictionary = en;

function detected() {
  const wanted = (navigator.languages ?? [navigator.language] ?? [])
    .filter(Boolean)
    .map((tag) => tag.toLowerCase().split('-')[0]);
  return wanted.find((code) => code in dictionaries) ?? 'en';
}

/// The stored choice, applied. Awaited before the shell is built, because a
/// window that draws itself in English and then again in Russian is two windows.
export async function loadLanguage() {
  try {
    choice = (await window.__TAURI__.core.invoke('get_language', {})) ?? followWindows;
  } catch {
    // A setting that cannot be read is a setting that was never made.
    choice = followWindows;
  }
  dictionary = dictionaries[choice] ?? dictionaries[detected()];
}

/// Stores the choice and starts the page over. Every string here is taken out of
/// the dictionary as the UI is built, so building it again is what applying a
/// language means; anything typed into a form goes with the page it was typed in.
export async function chooseLanguage(next) {
  await window.__TAURI__.core.invoke('set_language', { language: next }).catch(() => {});
  window.location.reload();
}

export const currentLanguage = () => choice;

export function t(key, values) {
  const text = dictionary[key] ?? en[key] ?? key;
  if (!values) {
    return text;
  }
  return Object.entries(values).reduce(
    (acc, [name, value]) => acc.replaceAll(`{${name}}`, value),
    text
  );
}

// Translates the declarative hooks: visible text, then the two attributes the
// form controls need instead of text content.
export function localize(root = document) {
  for (const element of root.querySelectorAll('[data-i18n]')) {
    element.textContent = t(element.dataset.i18n);
  }
  for (const element of root.querySelectorAll('[data-i18n-placeholder]')) {
    element.placeholder = t(element.dataset.i18nPlaceholder);
  }
  for (const element of root.querySelectorAll('[data-i18n-label]')) {
    element.setAttribute('aria-label', t(element.dataset.i18nLabel));
  }
}
