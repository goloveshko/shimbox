// The form widgets both screens build the same way. Nothing here knows about
// a particular mode.
import { localize, t } from './i18n.js';

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

/// One labelled block: its caption, its control, and whatever hint belongs
/// under it. The label still points at the control's id, so the field keeps its
/// accessible name wherever the form grid puts it.
export function field(key, control, ...children) {
  const block = document.createElement('div');
  block.className = 'field';
  block.append(labelFor(key, control), ...children);
  return block;
}

/// A checkbox whose text is the label itself, which makes the whole line
/// clickable and gives the control its accessible name without an id.
export function checkField(key, input, name) {
  const label = document.createElement('label');
  label.className = 'check';
  const text = document.createElement('span');
  if (name) {
    // A language names itself the same in every UI, so this one line is not a key.
    text.textContent = name;
  } else {
    text.dataset.i18n = key;
  }
  label.append(input, text);
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

/// `key` names what the button picks rather than which screen holds it, so the
/// destination of either mode shares one remembered directory, and the backend
/// opens the dialog there instead of where another field's dialog last ended.
export function withBrowseButton(input, command, key) {
  const row = document.createElement('div');
  row.className = 'row';

  const button = document.createElement('button');
  button.type = 'button';
  button.dataset.i18n = 'form.browse';
  button.addEventListener('click', async () => {
    const picked = await invoke(command, { key, path: input.value });
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

/// Whether two strings name the same folder, which is what decides if a
/// `workdir` has to be written at all: Windows ignores case and a trailing
/// separator, and the backend applies the same rule (3.4).
export function sameFolder(left, right) {
  const trim = (path) => path.replace(/[\\/]+$/, '').toLowerCase();
  return trim(left) === trim(right);
}

/// Sizes are read by eye, so one decimal is enough to tell 208 KB from 211 KB.
export function formatSize(bytes) {
  const kb = bytes / 1024;
  if (kb < 1) {
    return `${bytes} B`;
  }
  const mb = kb / 1024;
  if (mb < 1) {
    return `${Math.round(kb)} KB`;
  }
  const gb = mb / 1024;
  return gb < 1 ? `${mb.toFixed(1)} MB` : `${gb.toFixed(1)} GB`;
}

/// Fills a field from another one until the user edits it themselves. A folder
/// picked through the Browse button counts as an edit: it arrives as a `change`
/// the button dispatches, not as typing.
export function autofill(input, compute) {
  let edited = false;
  for (const event of ['input', 'change']) {
    input.addEventListener(event, () => {
      edited = true;
    });
  }
  return () => {
    if (!edited) {
      input.value = compute();
    }
  };
}

/// The one shape a message takes: a card over a frame that dims the window, so a
/// click anywhere around the card ends it — a larger, closer target than the
/// dismiss button. The body is the part that gives when content is taller than
/// the window, and the last action is focused so Enter always ends the message.
export function modalDialog() {
  const dialog = document.createElement('dialog');
  const card = document.createElement('div');
  card.className = 'dialog-card';
  const body = document.createElement('div');
  body.className = 'dialog-body';
  const actions = document.createElement('div');
  actions.className = 'dialog-actions';

  card.append(body, actions);
  dialog.append(card);
  dialog.addEventListener('click', (event) => {
    if (event.target === dialog) {
      dialog.close();
    }
  });

  dialog.openModal = (title, content, buttons) => {
    dialog.setAttribute('aria-label', title);
    body.replaceChildren(content);
    actions.replaceChildren(...buttons);
    // The content is filled long after the screen's one translation pass, so its
    // own buttons are localised on the way out.
    localize(dialog);
    dialog.showModal();
    buttons.at(-1)?.focus();
  };
  // A card that dims the whole window belongs to the whole window: inside a
  // screen it would be hidden along with that screen, and a tab click during a
  // result would take the message away with it.
  document.body.append(dialog);
  return dialog;
}

/// The dismiss action every message offers, so a card and its Close button are
/// built the same way wherever one is opened.
export function closeButton(dialog, key = 'dialog.close') {
  const button = document.createElement('button');
  button.type = 'button';
  button.dataset.i18n = key;
  button.addEventListener('click', () => dialog.close());
  return button;
}

/// `success` and `fail` rather than `show`, which a dialog already owns. A
/// destination that is not on `PATH` is nothing, so a result that wrote something
/// also comes with the ways to act on the folder it wrote into (6).
export function resultDialog() {
  const dialog = modalDialog();
  const card = dialog.querySelector('.dialog-card');

  function show(title, content, folder) {
    const buttons = folder
      ? [openFolderButton(folder), editPathButton(), closeButton(dialog)]
      : [closeButton(dialog)];
    dialog.openModal(title, content, buttons);
  }

  dialog.success = (folder, build) => {
    card.classList.remove('error');
    const content = document.createElement('div');
    build(content);
    show(t('result.title'), content, folder);
  };
  dialog.fail = (text, detail) => {
    card.classList.add('error');
    const content = document.createElement('div');
    content.textContent = detail ? `${text} ${detail}` : text;
    show(text, content);
  };
  return dialog;
}

/// The two actions a result offers besides dismissing itself. A destination that
/// is not on `PATH` is nothing, so the pair comes with the ways to act on it:
/// look at the folder, and add it where Windows looks a command name up (6).
function openFolderButton(folder) {
  const button = document.createElement('button');
  button.type = 'button';
  button.dataset.i18n = 'result.open';
  button.addEventListener('click', () => invoke('open_folder', { path: folder }));
  return button;
}

/// The `PATH` entry itself belongs to the system, so this only opens the window
/// that edits it; shimbox never writes the variable.
function editPathButton() {
  const button = document.createElement('button');
  button.type = 'button';
  button.dataset.i18n = 'result.edit';
  button.addEventListener('click', () => invoke('edit_path', {}));
  return button;
}
