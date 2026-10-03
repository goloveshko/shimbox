// Mode 1 of the spec (docs/techspec.md 5.1): one shim at a time.
import { t } from '../i18n.js';

const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);

let nextId = 0;

function textInput(placeholderKey) {
  const input = document.createElement('input');
  input.type = 'text';
  input.id = `single-${nextId++}`;
  input.dataset.i18nPlaceholder = placeholderKey;
  return input;
}

function labelFor(key, control) {
  const label = document.createElement('label');
  label.htmlFor = control.id;
  label.dataset.i18n = key;
  return label;
}

function note(key) {
  const element = document.createElement('p');
  element.className = 'note';
  element.dataset.i18n = key;
  return element;
}

function withBrowseButton(input, command) {
  const row = document.createElement('div');
  row.className = 'row';

  const button = document.createElement('button');
  button.type = 'button';
  button.dataset.i18n = 'single.browse';
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

function stemOf(path) {
  const name = path.split(/[\\/]/).pop() ?? '';
  return name.replace(/\.[^.]+$/, '');
}

export function renderSingle(root) {
  const form = document.createElement('form');

  const target = textInput('single.target.placeholder');
  form.append(labelFor('single.target', target), withBrowseButton(target, 'pick_file'));

  const alias = textInput('single.alias.placeholder');
  form.append(labelFor('single.alias', alias), alias);

  const args = textInput('single.arguments.placeholder');
  form.append(labelFor('single.arguments', args), args, note('single.arguments.help'));

  const variant = document.createElement('select');
  variant.id = `single-${nextId++}`;
  for (const value of ['auto', 'cli', 'gui']) {
    const option = document.createElement('option');
    option.value = value;
    option.dataset.i18n = `single.variant.${value}`;
    variant.append(option);
  }
  const detected = document.createElement('p');
  detected.className = 'note';
  form.append(labelFor('single.variant', variant), variant, detected);

  const workdir = textInput('single.workdir.placeholder');
  form.append(
    labelFor('single.workdir', workdir),
    withBrowseButton(workdir, 'pick_folder'),
    note('single.workdir.help')
  );

  let aliasEdited = false;
  alias.addEventListener('input', () => {
    aliasEdited = true;
  });

  target.addEventListener('change', async () => {
    const path = target.value.trim();
    if (!aliasEdited) {
      alias.value = stemOf(path);
    }
    if (!path) {
      detected.textContent = '';
      return;
    }
    const result = await invoke('detect_variant', { path });
    detected.textContent =
      result.subsystem === 'unknown'
        ? t('single.variant.unknown')
        : t('single.variant.detected', { subsystem: result.subsystem });
  });

  root.append(form);
}
