// Mode 2 of the spec (docs/techspec.md 5.2): a whole folder at once. No
// per-entry parameters, so the table is a list of names and nothing else.
import { t } from '../i18n.js';
import {
  checkField,
  field,
  folderOf,
  formatSize,
  invoke,
  labelFor,
  note,
  resultDialog,
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

/// A path long enough to push the table out of the window loses its middle,
/// not its end: the file name is the part that says which row is which.
function pathCell(path) {
  const folder = folderOf(path);
  const head = document.createElement('span');
  head.textContent = folder;
  const tail = document.createElement('span');
  tail.textContent = path.slice(folder.length);

  const row = document.createElement('div');
  row.className = 'path';
  row.append(head, tail);

  const element = document.createElement('td');
  element.title = path;
  element.append(row);
  return element;
}

function sizeCell(bytes) {
  const element = document.createElement('td');
  element.className = 'size';
  element.textContent = formatSize(bytes);
  return element;
}

/// Build strings carry a suffix like `(WinBuild.160101.0800)` that says nothing
/// about which file is newer, so the cell keeps the leading numbers readable
/// and puts the rest in the tooltip.
function versionCell(version) {
  const text = document.createElement('div');
  text.className = 'version';
  text.textContent = version ?? '';

  const element = document.createElement('td');
  element.title = version ?? '';
  element.append(text);
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

/// Files that would claim the same shim name are alternatives for one slot, so
/// they are given a shared `group` and the table treats them as a choice rather
/// than as a set of checkboxes. `preferred` keeps what the scan picked, which
/// the filter and `Select all` both fall back to. (5.2)
function groupRows(list) {
  const counts = new Map();
  for (const row of list) {
    row.group = row.name.toLowerCase();
    row.preferred = row.selected;
    counts.set(row.group, (counts.get(row.group) ?? 0) + 1);
  }
  for (const row of list) {
    row.grouped = counts.get(row.group) > 1;
  }
}

/// Keep the files the user had chosen across a rescan. A name keeps one row, so
/// a carried choice also clears the siblings that now claim the same shim name;
/// a name whose chosen file is gone from the results keeps whatever the scan
/// picked instead. (5.2)
function carry(list, chosen) {
  if (!chosen.size) {
    return;
  }
  const taken = new Set();
  for (const row of list) {
    if (chosen.has(row.path)) {
      taken.add(row.group);
    }
  }
  for (const row of list) {
    if (taken.has(row.group)) {
      row.selected = chosen.has(row.path);
    }
  }
}

export function renderBatch(root) {
  const form = document.createElement('form');

  const source = textInput('batch.source.placeholder');
  const destination = textInput('batch.destination.placeholder');
  form.append(
    field('batch.source', source, withBrowseButton(source, 'pick_folder')),
    field(
      'batch.destination',
      destination,
      withBrowseButton(destination, 'pick_folder')
    )
  );

  // The three lines that explain the scan belong to both folders above, so they
  // sit under them rather than being split between two fields.
  const recursive = document.createElement('input');
  recursive.type = 'checkbox';
  const help = document.createElement('div');
  help.className = 'field span-all';
  help.append(
    checkField('batch.recursive', recursive),
    note('batch.destination.help'),
    note('batch.scan.help'),
    note('batch.group.help')
  );
  form.append(help);

  const found = note();
  const selectedCount = note();
  const filter = textInput('batch.filter.placeholder');
  const selectAll = smallButton('batch.selectAll');
  const deselectAll = smallButton('batch.deselectAll');
  const caption = document.createElement('div');
  caption.className = 'caption';
  caption.append(
    labelFor('batch.filter', filter),
    filter,
    selectAll,
    deselectAll,
    found,
    selectedCount
  );

  const body = document.createElement('tbody');
  const table = document.createElement('table');
  const head = document.createElement('tr');
  for (const key of [
    'batch.column.name',
    'batch.column.path',
    'batch.column.version',
    'batch.column.size',
    'batch.column.type',
  ]) {
    const label = document.createElement('th');
    label.dataset.i18n = key;
    label.className = key.split('.')[2];
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
  const result = resultDialog();

  // The scroll and the card belong to the wrapper, not the table: a sticky
  // header only stays put inside a box that actually scrolls.
  const tableWrap = document.createElement('div');
  tableWrap.className = 'table-wrap';
  tableWrap.append(table);

  const actions = document.createElement('div');
  actions.className = 'actions';
  actions.append(generate, progress);

  root.append(form, caption, tableWrap, actions, result);

  let rows = [];
  // What is on screen right now, and the checkbox each visible row owns. The
  // selection itself lives on the rows, so filtering cannot lose it.
  let shown = [];
  // Whether the table is empty because nothing was scanned yet or because a scan
  // came back with nothing in it. Only the second has anything to report.
  let scanned = false;
  const checks = new Map();

  // The table and its toolbar belong to a result, so the screen is drawn in that
  // state from the start rather than showing an empty one that the first click on
  // the toolbar would then take away.
  show();

  function show() {
    const needle = filter.value.trim().toLowerCase();
    shown = rows.filter(
      (row) => !needle || `${row.name} ${row.path}`.toLowerCase().includes(needle)
    );
    body.replaceChildren();
    checks.clear();

    for (const row of shown) {
      const check = document.createElement('input');
      check.type = 'checkbox';
      check.checked = row.selected;
      check.dataset.path = row.path;
      check.setAttribute('aria-label', row.name);
      check.addEventListener('change', () => choose(row));
      checks.set(row, check);

      const entry = document.createElement('tr');
      if (row.grouped) {
        entry.className = 'grouped';
      }
      entry.append(
        cell(check),
        cell(row.name),
        pathCell(row.path),
        versionCell(row.version),
        sizeCell(row.size),
        cell(t(`variant.${row.variant}`))
      );
      body.append(entry);
    }

    tableWrap.hidden = rows.length === 0;
    caption.hidden = rows.length === 0;
    // The table is the scan's answer, so when there is no table there has to be
    // a line saying so; before the first scan the screen stays quiet.
    if (scanned && rows.length === 0) {
      progress.textContent = t('batch.empty');
    }
    found.textContent = needle
      ? t('batch.matching', { shown: shown.length, count: rows.length })
      : t('batch.found', { count: rows.length });
    // The buttons only reach the rows the filter leaves visible, so say what
    // is actually going to be written.
    tally();
  }

  function tally() {
    const count = rows.filter((row) => row.selected).length;
    selectedCount.textContent = t('batch.selected', { count });
    // Neither button has anything to do when the rows on screen already match it:
    // `Select all` over a chosen set and `Select none` over an empty one both
    // leave the table as it was, and no rows at all settles neither way.
    selectAll.disabled = everyNameChosen();
    deselectAll.disabled = !shown.some((row) => row.selected);
    // The scan can fill the table before any destination is chosen, and writing
    // needs both a folder and a row, so the button waits for them instead of
    // answering an incomplete request with an error (5.2).
    generate.disabled = !destination.value.trim() || count === 0;
  }

  /// Checking a row takes the shim name, which unchecks every other file that
  /// would have claimed it, filtered out or not.
  function choose(row) {
    row.selected = checks.get(row).checked;
    if (row.selected) {
      for (const sibling of rows) {
        if (sibling === row || sibling.group !== row.group) {
          continue;
        }
        sibling.selected = false;
        const other = checks.get(sibling);
        if (other) {
          other.checked = false;
        }
      }
    }
    tally();
  }

  /// `Select all` cannot mean every row: one name gets one shim. It means the
  /// row already picked, else the one the scan picked, else the first visible
  /// one, and `Select none` clears the same set of rows the filter leaves.
  function selectVisible(state) {
    const picked = new Set();
    if (state) {
      for (const group of new Set(shown.map((row) => row.group))) {
        const members = shown.filter((row) => row.group === group);
        const winner =
          members.find((row) => row.selected) ??
          members.find((row) => row.preferred) ??
          members[0];
        picked.add(winner.path);
      }
    }
    for (const row of shown) {
      row.selected = picked.has(row.path);
    }
    show();
  }

  /// Whether every shim name the filter leaves visible already has one row
  /// chosen, which is the case where `Select all` would change nothing: a name
  /// with a choice keeps it and a name without one has none to gain.
  function everyNameChosen() {
    const chosen = new Set();
    for (const row of shown) {
      if (row.selected) {
        chosen.add(row.group);
      }
    }
    return shown.every((row) => chosen.has(row.group));
  }

  async function rescan() {
    const path = source.value.trim();
    if (!path) {
      rows = [];
      scanned = false;
      show();
      return;
    }
    progress.textContent = t('batch.scanning');
    // A scan answers with its own preselection, but here it only follows a
    // folder the user edited, so the choices made by hand are kept by path.
    const chosen = new Set(rows.filter((row) => row.selected).map((row) => row.path));
    try {
      rows = await invoke('scan', {
        source: path,
        destination: destination.value,
        recursive: recursive.checked,
      });
      groupRows(rows);
      carry(rows, chosen);
      scanned = true;
    } catch (failure) {
      rows = [];
      scanned = false;
      result.fail(t(`error.${failure.code}`), failure.detail);
    } finally {
      progress.textContent = '';
    }
    show();
  }

  // Both fields feed the scan: the destination is what gets left out of it.
  for (const field of [source, destination]) {
    field.addEventListener('change', rescan);
  }
  destination.addEventListener('input', () => {
    progress.textContent = '';
    tally();
  });
  recursive.addEventListener('change', () => {
    if (source.value.trim()) {
      rescan();
    }
  });

  filter.addEventListener('input', show);

  selectAll.addEventListener('click', () => selectVisible(true));
  deselectAll.addEventListener('click', () => selectVisible(false));

  generate.addEventListener('click', async () => {
    // The selection lives on the rows, so a filtered-out one is still written;
    // the counter next to the filter is what says how many there are.
    const targets = rows.filter((row) => row.selected).map((row) => row.path);
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
      tally();
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
  result.success(folder, (box) => {
    box.append(
      t(summary.created.length ? 'batch.created' : 'batch.created.none', {
        count: summary.created.length,
        destination: folder,
      })
    );
    if (folder) {
      box.append(
        document.createElement('br'),
        list(summary.created.map((path) => path.slice(folder.length + 1))),
        t('result.path')
      );
    }
    if (summary.skipped.length) {
      box.append(
        document.createElement('br'),
        t('batch.skipped'),
        list(summary.skipped.map((row) => `${row.path} - ${t(`batch.skip.${row.reason}`)}`))
      );
    }
  });
}
