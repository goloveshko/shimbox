// Mode 1 of the spec (docs/techspec.md 5.1): one shim at a time.
import { t, localize } from '../i18n.js';

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

function folderOf(path) {
  const separator = Math.max(path.lastIndexOf('\\'), path.lastIndexOf('/'));
  return separator > 0 ? path.slice(0, separator) : '';
}

/// Fills a field from the target until the user edits it themselves.
function autofill(input, compute) {
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

function resultPanel() {
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
  return panel;
}

export function renderSingle(root) {
  const form = document.createElement('form');

  const target = textInput('single.target.placeholder');
  form.append(labelFor('single.target', target), withBrowseButton(target, 'pick_file'));

  const destination = textInput('single.destination.placeholder');
  form.append(
    labelFor('single.destination', destination),
    withBrowseButton(destination, 'pick_folder'),
    note('single.destination.help')
  );

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
  const workdirNote = document.createElement('p');
  workdirNote.className = 'note';
  form.append(labelFor('single.workdir', workdir), withBrowseButton(workdir, 'pick_folder'), workdirNote);

  const generate = document.createElement('button');
  generate.type = 'submit';
  generate.dataset.i18n = 'single.generate';
  form.append(generate);

  const result = resultPanel();
  root.append(form, result);

  const fillAlias = autofill(alias, () => stemOf(target.value));
  const fillDestination = autofill(destination, () => folderOf(target.value));

  // The default is the runner's, not a value we write into the config: naming
  // the folder here keeps it visible without pinning it into every shim.
  function showWorkdirDefault() {
    const folder = workdir.value.trim() ? '' : folderOf(target.value);
    workdirNote.textContent = folder
      ? t('single.workdir.default', { folder })
      : t('single.workdir.help');
  }

  showWorkdirDefault();

  target.addEventListener('change', async () => {
    const path = target.value.trim();
    fillAlias();
    fillDestination();
    showWorkdirDefault();
    result.clear();
    if (!path) {
      detected.textContent = '';
      return;
    }
    const found = await invoke('detect_variant', { path });
    detected.textContent = t(`single.detected.${found.subsystem}`);
  });

  workdir.addEventListener('input', showWorkdirDefault);

  for (const input of [destination, alias, args, variant, workdir]) {
    input.addEventListener('input', result.clear);
  }

  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    generate.disabled = true;
    try {
      const created = await invoke('generate', {
        request: {
          target: target.value,
          destination: destination.value,
          alias: alias.value,
          arguments: args.value,
          variant: variant.value,
          workdir: workdir.value,
        },
      });
      showCreated(result, created);
    } catch (failure) {
      result.fail(t(`error.${failure.code}`), failure.detail);
    } finally {
      generate.disabled = false;
    }
  });
}

function showCreated(result, created) {
  result.clear();

  const folder = folderOf(created.exe);
  const lines = [
    t('single.result', { name: created.exe.slice(folder.length + 1), destination: folder }),
    t('single.result.variant', { variant: t(`single.variant.${created.variant}`) }),
    t('single.result.path'),
  ];
  for (const line of lines) {
    result.append(line, document.createElement('br'));
  }

  const open = document.createElement('button');
  open.type = 'button';
  open.dataset.i18n = 'single.result.open';
  open.addEventListener('click', () => invoke('open_folder', { path: folder }));
  result.append(open);
  localize(result);
  result.hidden = false;
}
