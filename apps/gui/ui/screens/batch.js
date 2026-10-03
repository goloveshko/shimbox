// Mode 2 of the spec (docs/techspec.md 5.2): a whole folder at once. No
// per-entry parameters, so the table is a list of names and nothing else.
import { t } from '../i18n.js';
import {
  folderOf,
  invoke,
  labelFor,
  messagePanel,
  nextFieldId,
  note,
  openFolderButton,
  textInput,
  withBrowseButton,
} from '../form.js';

function cell(content, className) {
  const element = document.createElement('td');
  if (typeof content === 'string') {
    element.textContent = content;
  } else {
    element.append(content);
  }
  if (className) {
    element.className = className;
  }
  return element;
}

/// A path long enough to push the table out of the window is shown shortened,
/// with the full text kept in the tooltip.
function pathCell(path) {
  const shown = document.createElement('span');
  shown.textContent = path;
  const element = cell(shown, 'path');
  element.title = path;
  return element;
}

function list(items) {
  const element = document.createElement('ul');
  for (const item of items) {
    const entry = document.createElement('li');
    entry.textContent = item;
    element.append(entry);
  }
  return element;
}

export function renderBatch(root) {
  const form = document.createElement('form');

  const source = textInput('batch.source.placeholder');
  form.append(labelFor('batch.source', source), withBrowseButton(source, 'pick_folder'));

  const recursive = document.createElement('input');
  recursive.type = 'checkbox';
  recursive.id = nextFieldId();
  const recursiveRow = document.createElement('div');
  recursiveRow.className = 'checkbox';
  recursiveRow.append(recursive);
  form.append(labelFor('batch.recursive', recursive), recursiveRow);

  const destination = textInput('batch.destination.placeholder');
  form.append(
    labelFor('batch.destination', destination),
    withBrowseButton(destination, 'pick_folder'),
    note('batch.destination.help')
  );

  const found = note();
  const selectAll = smallButton('batch.selectAll');
  const deselectAll = smallButton('batch.deselectAll');
  const caption = document.createElement('div');
  caption.className = 'caption';
  caption.append(selectAll, deselectAll, found);

  const body = document.createElement('tbody');
  const table = document.createElement('table');
  const head = document.createElement('tr');
  for (const key of ['batch.column.name', 'batch.column.path', 'batch.column.type']) {
    const label = document.createElement('th');
    label.dataset.i18n = key;
    head.append(label);
  }
  head.prepend(document.createElement('th'));
  const thead = document.createElement('thead');
  thead.append(head);
  table.append(thead, body);

  const generate = document.createElement('button');
  generate.type = 'button';
  generate.className = 'primary';
  generate.dataset.i18n = 'batch.generate';
  generate.disabled = true;

  const progress = note();
  const result = messagePanel();
  root.append(form, caption, table, generate, progress, result);

  let rows = [];

  function show() {
    body.replaceChildren();
    for (const row of rows) {
      const check = document.createElement('input');
      check.type = 'checkbox';
      check.checked = true;
      check.dataset.path = row.path;
      check.setAttribute('aria-label', row.name);
      const entry = document.createElement('tr');
      entry.append(
        cell(check),
        cell(row.name),
        pathCell(row.path),
        cell(t(`variant.${row.variant}`))
      );
      body.append(entry);
    }
    table.hidden = rows.length === 0;
    caption.hidden = rows.length === 0;
    generate.disabled = rows.length === 0;
    found.textContent = t('batch.found', { count: rows.length });
  }

  async function rescan() {
    result.clear();
    const path = source.value.trim();
    if (!path) {
      rows = [];
      show();
      return;
    }
    progress.textContent = t('batch.scanning');
    try {
      rows = await invoke('scan', { source: path, recursive: recursive.checked });
    } catch (failure) {
      rows = [];
      result.fail(t(`error.${failure.code}`), failure.detail);
    } finally {
      progress.textContent = '';
    }
    show();
  }

  source.addEventListener('change', rescan);
  destination.addEventListener('change', result.clear);
  destination.addEventListener('input', () => {
    result.clear();
    progress.textContent = '';
  });
  recursive.addEventListener('change', () => {
    if (source.value.trim()) {
      rescan();
    }
  });

  const select = (state) => () => {
    for (const check of body.querySelectorAll('input[type=checkbox]')) {
      check.checked = state;
    }
  };
  selectAll.addEventListener('click', select(true));
  deselectAll.addEventListener('click', select(false));

  generate.addEventListener('click', async () => {
    const targets = [...body.querySelectorAll('input[type=checkbox]')]
      .filter((check) => check.checked)
      .map((check) => check.dataset.path);
    if (!targets.length) {
      result.fail(t('batch.nothing'), '');
      return;
    }
    generate.disabled = true;
    const unlisten = await window.__TAURI__.event.listen('batch-progress', (event) => {
      progress.textContent = t('batch.progress', event.payload);
    });
    try {
      const summary = await invoke('generate_batch', {
        request: { targets, destination: destination.value },
      });
      showSummary(result, summary);
    } catch (failure) {
      result.fail(t(`error.${failure.code}`), failure.detail);
    } finally {
      unlisten();
      progress.textContent = '';
      generate.disabled = false;
    }
  });
}

function smallButton(key) {
  const button = document.createElement('button');
  button.type = 'button';
  button.dataset.i18n = key;
  return button;
}

function showSummary(result, summary) {
  const folder = summary.created.length ? folderOf(summary.created[0]) : '';
  result.show((panel) => {
    panel.append(
      t(summary.created.length ? 'batch.created' : 'batch.created.none', {
        count: summary.created.length,
        destination: folder,
      })
    );
    if (folder) {
      panel.append(
        document.createElement('br'),
        list(summary.created.map((path) => path.slice(folder.length + 1))),
        t('result.path'),
        document.createElement('br'),
        openFolderButton(folder)
      );
    }
    if (summary.skipped.length) {
      panel.append(
        document.createElement('br'),
        t('batch.skipped'),
        list(summary.skipped.map((row) => `${row.path} - ${t(`batch.skip.${row.reason}`)}`))
      );
    }
  });
}
