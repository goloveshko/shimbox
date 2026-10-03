// Resolves the UI language. Adding a language means dropping a file into
// locales/ and registering it here; nothing else looks at the list.
import en from './locales/en.js';

const languages = { en };

const wanted = (navigator.languages ?? [navigator.language] ?? [])
  .filter(Boolean)
  .map((tag) => tag.toLowerCase().split('-')[0]);

export const locale = wanted.find((code) => code in languages) ?? 'en';

const dictionary = languages[locale];

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
