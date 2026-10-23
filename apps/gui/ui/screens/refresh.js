// Mode 3 of the spec (docs/techspec.md 5.7): the shims this machine already holds,
// brought up to the runners this build embeds. One folder, or every folder on PATH
// while no folder is named, and one command that rewrites a runner and nothing else.
import { setText, t } from '../i18n.js';
import {
  cell,
  field,
  invoke,
  list,
  nameOf,
  note,
  pathCell,
  resultDialog,
  textInput,
  withBrowseButton,
} from '../form.js';

export function renderRefresh(root) {
  const form = document.createElement('form');
  // One card, as in the other two modes: the folder at its top, the list in the
  // middle, the command on its bottom edge.
  const fields = document.createElement('div');
  fields.className = 'fields';
  form.append(fields);

  const folder = textInput('refresh.folder.placeholder');
  fields.append(field('refresh.folder', folder, withBrowseButton(folder, 'pick_folder', 'shims')));

  const help = document.createElement('div');
  help.className = 'field span-all';
  help.append(note('refresh.folder.help'), note('refresh.keeps.help'));
  fields.append(help);

  const found = note();
  const chosen = note();
  const caption = document.createElement('div');
  caption.className = 'caption';
  caption.append(found, chosen);

  const body = document.createElement('tbody');
  const table = document.createElement('table');
  const head = document.createElement('tr');
  for (const key of [
    'refresh.column.name',
    'refresh.column.path',
    'refresh.column.was',
    'refresh.column.type',
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

  const update = document.createElement('button');
  update.type = 'button';
  update.className = 'primary';
  update.dataset.i18n = 'refresh.update';
  update.disabled = true;

  const empty = note();
  const result = resultDialog();

  // The scroll and the card belong to the wrapper, not the table: a sticky header
  // only stays put inside a box that actually scrolls.
  const tableWrap = document.createElement('div');
  tableWrap.className = 'table-wrap';
  tableWrap.append(table);

  const actions = document.createElement('div');
  actions.className = 'actions';
  actions.append(update, empty);

  form.append(caption, tableWrap, actions);
  root.append(form);

  let rows = [];
  // The scan runs on its own when the screen is built, so the line that says the
  // answer was nothing has to wait for one answer to have come in.
  let scanned = false;
  // The checkbox each row owns, so a tick is read back from the box the table drew
  // rather than from a second copy of the answer.
  const checks = new Map();

  show();

  function show() {
    body.replaceChildren();
    checks.clear();

    for (const row of rows) {
      const check = document.createElement('input');
      check.type = 'checkbox';
      check.checked = row.selected;
      check.setAttribute('aria-label', row.name);
      check.addEventListener('change', () => {
        row.selected = check.checked;
        tally();
      });
      checks.set(row, check);

      // A pair made before the stamp existed says nothing about its own version,
      // and the cell that holds no answer is the row's honest one.
      const was = document.createElement('td');
      was.className = 'was';
      was.textContent = row.was || t('refresh.unstamped');

      const entry = document.createElement('tr');
      entry.append(
        cell(check),
        cell(row.name),
        pathCell(row.exe),
        was,
        setText(cell('', 'type'), `variant.${row.variant}`)
      );
      body.append(entry);
    }

    tableWrap.hidden = rows.length === 0;
    caption.hidden = rows.length === 0;
    setText(found, 'refresh.found', { count: rows.length });
    tally();
  }

  function tally() {
    const count = rows.filter((row) => row.selected).length;
    setText(chosen, 'refresh.selected', { count });
    setText(empty, scanned && !rows.length ? 'refresh.empty' : null);
    update.disabled = count === 0;
  }

  /// One field, two questions: the folder named is the one asked about, and a field
  /// left empty asks the machine - every folder it can already reach by name (5.7).
  async function rescan() {
    try {
      rows = await invoke('scan_shims', { folder: folder.value.trim() });
    } catch (failure) {
      rows = [];
      result.fail(`error.${failure.code}`, failure.detail);
    }
    scanned = true;
    show();
  }

  folder.addEventListener('change', rescan);

  update.addEventListener('click', async () => {
    const configs = rows.filter((row) => row.selected).map((row) => row.config);
    update.disabled = true;
    try {
      showSummary(result, await invoke('refresh_runners', { request: { configs } }));
      await rescan();
    } catch (failure) {
      result.fail(`error.${failure.code}`, failure.detail);
      tally();
    }
  });

  // The list is worth having before the tab is opened, and a scan of `PATH` reads
  // one small file per pair, so it is the screen that arrives ready rather than a
  // click that asks for it.
  rescan();

  return { folder };
}

function showSummary(result, summary) {
  result.success('', (box) => {
    const names = summary.refreshed.map(nameOf);
    box.append(t(names.length ? 'refresh.done' : 'refresh.done.none', { count: names.length }));
    if (names.length) {
      box.append(document.createElement('br'), list(names));
    }
    if (summary.failed.length) {
      box.append(
        document.createElement('br'),
        t('refresh.failed'),
        list(summary.failed.map((row) => `${t(`error.${row.code}`)} ${row.detail ?? ''}`.trim()))
      );
    }
  });
}
