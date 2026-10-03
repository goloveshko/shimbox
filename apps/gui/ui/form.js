// The form widgets both screens build the same way. Nothing here knows about
// a particular mode.
import { localize } from './i18n.js';

export const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);

let nextId = 0;

/// Ids are only needed to bind a `<label>` to a control the caller built by
/// hand, such as a `select` or a checkbox.
export function nextFieldId() {
  return `field-${nextId++}`;
}

export function textInput(placeholderKey) {
  const input = document.createElement('input');
  input.type = 'text';
  input.id = nextFieldId();
  input.dataset.i18nPlaceholder = placeholderKey;
  return input;
}

export function labelFor(key, control) {
  const label = document.createElement('label');
  label.htmlFor = control.id;
  label.dataset.i18n = key;
  return label;
}

/// A hint line under a field. Pass no key when the caller writes its text.
export function note(key) {
  const element = document.createElement('p');
  element.className = 'note';
  if (key) {
    element.dataset.i18n = key;
  }
  return element;
}

export function withBrowseButton(input, command) {
  const row = document.createElement('div');
  row.className = 'row';

  const button = document.createElement('button');
  button.type = 'button';
  button.dataset.i18n = 'form.browse';
  button.addEventListener('click', async () => {
    const picked = await invoke(command, {});
    if (picked) {
      input.value = picked;
      input.dispatchEvent(new Event('change'));
    }
  });

  row.append(input, button);
  return row;
}

export function stemOf(path) {
  const name = path.split(/[\\/]/).pop() ?? '';
  return name.replace(/\.[^.]+$/, '');
}

export function folderOf(path) {
  const separator = Math.max(path.lastIndexOf('\\'), path.lastIndexOf('/'));
  return separator > 0 ? path.slice(0, separator) : '';
}

/// Fills a field from another one until the user edits it themselves.
export function autofill(input, compute) {
  let edited = false;
  input.addEventListener('input', () => {
    edited = true;
  });
  return () => {
    if (!edited) {
      input.value = compute();
    }
  };
}

/// The message area below a form: a plain paragraph that can be filled with
/// text or with built-up content, and cleared by the form on any change.
export function messagePanel() {
  const panel = document.createElement('p');
  panel.className = 'result';
  panel.hidden = true;

  panel.clear = () => {
    panel.hidden = true;
    panel.textContent = '';
    panel.classList.remove('error');
  };
  panel.fail = (text, detail) => {
    panel.clear();
    panel.classList.add('error');
    panel.textContent = detail ? `${text} ${detail}` : text;
    panel.hidden = false;
  };
  panel.show = (build) => {
    panel.clear();
    build(panel);
    localize(panel);
    panel.hidden = false;
  };
  return panel;
}

/// The one action both result panels offer: a destination that is not on
/// `PATH` is nothing, so being able to look at it matters most (6).
export function openFolderButton(folder) {
  const button = document.createElement('button');
  button.type = 'button';
  button.dataset.i18n = 'result.open';
  button.addEventListener('click', () => invoke('open_folder', { path: folder }));
  return button;
}
