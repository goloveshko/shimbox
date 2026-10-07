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

/// The dictionary the keys are read from, and the claim the document makes about
/// itself to anything that reads it instead of looking at the screen.
function apply(next) {
  const code = next in dictionaries ? next : detected();
  dictionary = dictionaries[code];
  document.documentElement.lang = code;
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
  apply(choice);
}

/// Stores the choice and rewrites the screen that stands in front of the person.
/// Every string the window shows is left on its node by key, so a language is one
/// pass over the document rather than a new document, and what a form holds stays
/// in it - the way a theme already applies.
export async function chooseLanguage(next) {
  await window.__TAURI__.core.invoke('set_language', { language: next }).catch(() => {});
  choice = next;
  apply(next);
  localize();
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

/// The values a key needs to become its sentence, kept beside the key so the next
/// pass can rebuild the same sentence in another language.
const valuesOf = (element) =>
  element.dataset.i18nValues ? JSON.parse(element.dataset.i18nValues) : undefined;

/// Leaves a key on a node under the hook `where` names and returns its text. A
/// string a screen only knows while it runs is written this way instead of by
/// assigning `t(...)` to it, which is what lets a language pass find it again.
/// No key means the node shows nothing, and loses the key it carried: a screen
/// that cleared a line on purpose would have it filled back by the next pass.
function bind(element, where, key, values) {
  if (!key) {
    delete element.dataset[where];
    delete element.dataset.i18nValues;
    return '';
  }
  element.dataset[where] = key;
  if (values) {
    element.dataset.i18nValues = JSON.stringify(values);
  }
  return t(key, values);
}

export function setText(element, key, values) {
  element.textContent = bind(element, 'i18n', key, values);
  return element;
}

export function setLabel(element, key, values) {
  element.setAttribute('aria-label', bind(element, 'i18nLabel', key, values));
  return element;
}

export function setTitle(element, key, values) {
  element.title = bind(element, 'i18nTitle', key, values);
  return element;
}

// Rewrites every node a key was left on: visible text first, then the three
// attributes a control carries its name in instead of as content. A key that is
// not a key answers with itself, so a string a caller had already resolved
// survives the pass unchanged rather than being emptied.
export function localize(root = document) {
  for (const element of root.querySelectorAll('[data-i18n]')) {
    element.textContent = t(element.dataset.i18n, valuesOf(element));
  }
  for (const element of root.querySelectorAll('[data-i18n-placeholder]')) {
    element.placeholder = t(element.dataset.i18nPlaceholder, valuesOf(element));
  }
  for (const element of root.querySelectorAll('[data-i18n-label]')) {
    element.setAttribute('aria-label', t(element.dataset.i18nLabel, valuesOf(element)));
  }
  for (const element of root.querySelectorAll('[data-i18n-title]')) {
    element.title = t(element.dataset.i18nTitle, valuesOf(element));
  }
}
