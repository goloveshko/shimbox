// Mode 2 of the spec (docs/techspec.md 5.2): a whole folder at once. No
// per-entry parameters, so the table is a list of names and nothing else.
import { setText, t } from '../i18n.js';
import {
  cell,
  checkField,
  field,
  folderOf,
  formatSize,
  invoke,
  labelFor,
  list,
  nameOf,
  note,
  pathCell,
  prefillFolder,
  resultDialog,
  stemOf,
  textInput,
  withBrowseButton,
} from '../form.js';

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
  // One card for the whole screen, as in mode 1: the folders at its top, the
  // scan in the middle, the command on its bottom edge.
  const fields = document.createElement('div');
  fields.className = 'fields';
  form.append(fields);

  const source = textInput('batch.source.placeholder');
  const destination = textInput('batch.destination.placeholder');
  fields.append(
    field('batch.source', source, withBrowseButton(source, 'pick_folder', 'source')),
    field(
      'batch.destination',
      destination,
      withBrowseButton(destination, 'pick_folder', 'destination')
    )
  );
  // The same remembered folder mode 1 opens with, because the two modes write
  // into one place (5).
  prefillFolder(destination, 'destination');

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
  fields.append(help);

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

  form.append(caption, tableWrap, actions);
  root.append(form);

  let rows = [];
  // What is on screen right now, and the checkbox each visible row owns. The
  // selection itself lives on the rows, so filtering cannot lose it.
  let shown = [];
  // Whether the table is empty because nothing was scanned yet or because a scan
  // came back with nothing in it. Only the second has anything to report.
  let scanned = false;
  // How many scans have been asked for, so only the newest answer is drawn.
  let scans = 0;
  // Whether a batch is being written right now: the button holds its own state
  // for the length of the request, which the counter next to the filter is not
  // part of and cannot be trusted to leave alone.
  let busy = false;
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
        setText(cell(''), `variant.${row.variant}`)
      );
      body.append(entry);
    }

    tableWrap.hidden = rows.length === 0;
    caption.hidden = rows.length === 0;
    // The table is the scan's answer, so when there is no table there has to be
    // a line saying so; before the first scan the screen stays quiet.
    if (scanned && rows.length === 0) {
      setText(progress, 'batch.empty');
    }
    if (needle) {
      setText(found, 'batch.matching', { shown: shown.length, count: rows.length });
    } else {
      setText(found, 'batch.found', { count: rows.length });
    }
    // The buttons only reach the rows the filter leaves visible, so say what
    // is actually going to be written.
    tally();
  }

  function tally() {
    const count = rows.filter((row) => row.selected).length;
    setText(selectedCount, 'batch.selected', { count });
    // Neither button has anything to do when the rows on screen already match it:
    // `Select all` over a chosen set and `Select none` over an empty one both
    // leave the table as it was, and no rows at all settles neither way.
    selectAll.disabled = everyNameChosen();
    deselectAll.disabled = !shown.some((row) => row.selected);
    // The scan can fill the table before any destination is chosen, and writing
    // needs both a folder and a row, so the button waits for them instead of
    // answering an incomplete request with an error (5.2). A batch already under
    // way is not a row count: the ticks change while it runs, and a second request
    // over the same rows would write the same shims twice.
    generate.disabled = busy || !destination.value.trim() || count === 0;
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

  /// `Select all` cannot mean every row: one name gets one shim, across the whole
  /// table rather than only the part the filter leaves. A name a hidden row already
  /// holds keeps that row, so the two files of one name are never both ticked and
  /// the batch is never asked to write one shim twice. `Select none` clears the
  /// rows the filter leaves, which is all a person can see to mean.
  function selectVisible(state) {
    const picked = new Set();
    if (state) {
      for (const group of new Set(shown.map((row) => row.group))) {
        const claimed = rows.find((row) => row.group === group && row.selected);
        if (claimed) {
          picked.add(claimed.path);
          continue;
        }
        const members = shown.filter((row) => row.group === group);
        const winner = members.find((row) => row.preferred) ?? members[0];
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
  /// with a choice keeps it and a name without one has none to gain. The choice
  /// counts wherever it sits, because a hidden row of the same name is the answer
  /// [`selectVisible`] would reach for.
  function everyNameChosen() {
    const chosen = new Set(rows.filter((row) => row.selected).map((row) => row.group));
    return shown.every((row) => chosen.has(row.group));
  }

  async function rescan() {
    // Two scans can be in the air at once, since a folder is edited while the
    // first one still runs. The later one is the folder the field names now, so
    // the earlier answer is dropped rather than drawn over it.
    const scan = ++scans;
    const path = source.value.trim();
    if (!path) {
      // This is the newest scan and it has nothing to ask for, so the note the
      // superseded one was holding is cleared here rather than left up forever.
      setText(progress, null);
      rows = [];
      scanned = false;
      show();
      return;
    }
    setText(progress, 'batch.scanning');
    // A scan answers with its own preselection, but here it only follows a
    // folder the user edited, so the choices made by hand are kept by path.
    const chosen = new Set(rows.filter((row) => row.selected).map((row) => row.path));
    try {
      const found = await invoke('scan', {
        source: path,
        destination: destination.value,
        recursive: recursive.checked,
      });
      if (scan !== scans) {
        return;
      }
      rows = found;
      groupRows(rows);
      carry(rows, chosen);
      scanned = true;
    } catch (failure) {
      if (scan !== scans) {
        return;
      }
      rows = [];
      scanned = false;
      result.fail(`error.${failure.code}`, failure.detail);
    }
    if (scan !== scans) {
      return;
    }
    setText(progress, null);
    show();
  }

  // Both fields feed the scan: the destination is what gets left out of it.
  for (const input of [source, destination]) {
    input.addEventListener('change', rescan);
  }

  /// Dropped files name what to shim, not only where to look, so they take the
  /// choice over the scan's own preselection - one row per shim name, which is
  /// the rule checking a row already applies (5.2). The scan is run here rather
  /// than left to the field's `change`, because the ticks have to follow it
  /// instead of racing it.
  async function dropSource(folder, paths) {
    source.value = folder;
    await rescan();
    if (!paths.length) {
      return;
    }
    const key = (path) => path.replace(/\//g, '\\').toLowerCase();
    const wanted = new Set(paths.map(key));
    const winners = new Map();
    for (const row of rows) {
      if (wanted.has(key(row.path)) && !winners.has(row.group)) {
        winners.set(row.group, row);
      }
    }
    for (const row of rows) {
      row.selected = winners.get(row.group)?.path === row.path;
    }
    show();
  }
  destination.addEventListener('input', () => {
    setText(progress, null);
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
    busy = true;
    tally();
    const unlisten = await window.__TAURI__.event.listen('batch-progress', (event) => {
      setText(progress, 'batch.progress', event.payload);
    });
    try {
      const summary = await invoke('generate_batch', {
        request: { targets, destination: destination.value },
      });
      // A shim that is written is not going to be written again, and the table is
      // what the person reads after the dialog closes: the rows that made it lose
      // their tick, while a row that failed or was skipped keeps it and is already
      // selected for the retry. A written path is named by the same stem its source
      // row claims its group by, so the two sides have one key (5.2).
      const written = new Set(summary.created.map((path) => stemOf(path).toLowerCase()));
      for (const row of rows) {
        if (written.has(row.group)) {
          row.selected = false;
        }
      }
      show();
      showSummary(result, summary);
    } catch (failure) {
      result.fail(`error.${failure.code}`, failure.detail);
    } finally {
      unlisten();
      busy = false;
      setText(progress, null);
      tally();
    }
  });

  return { source, destination, dropSource };
}

function smallButton(key) {
  const button = document.createElement('button');
  button.type = 'button';
  button.dataset.i18n = key;
  return button;
}

function showSummary(result, summary) {
  const folder = summary.created.length ? folderOf(summary.created[0]) : '';
  result.success(folder, (box, onPath) => {
    box.append(
      t(summary.created.length ? 'batch.created' : 'batch.created.none', {
        count: summary.created.length,
        destination: folder,
      })
    );
    if (folder) {
      box.append(
        document.createElement('br'),
        list(summary.created.map(nameOf)),
        t(onPath ? 'result.path.on' : 'result.path')
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
