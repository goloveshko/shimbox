// The form widgets both screens build the same way. Nothing here knows about
// a particular mode.
import { localize, setLabel, setText, setTitle, t } from './i18n.js';

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
  // A screen writes its keys once, when it is built, and a field added later -
  // a row of the environment panel - is not there to see that. The hint is
  // written with the key so the control is right the moment it is made, and the
  // key is what a later language rewrites it from.
  input.placeholder = t(placeholderKey);
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

/// A push button whose caption is a key. `small` is the one that acts on a single
/// line of a list rather than on the screen.
export function button(key, className) {
  const element = document.createElement('button');
  element.type = 'button';
  element.dataset.i18n = key;
  element.className = className ?? '';
  return element;
}

/// Segoe's folder glyph, the shape the system's own file dialogs draw. Written as
/// an escape because a private-use character does not survive an editor.
export const folder = '\uE8B7';

/// A button that says what it does by shape rather than by word, so the name it
/// carries is its accessible one and its tooltip, not its caption.
export function iconButton(glyph, key, className) {
  const element = document.createElement('button');
  element.type = 'button';
  element.className = className ? `icon ${className}` : 'icon';
  element.textContent = glyph;
  setLabel(element, key);
  return setTitle(element, key);
}

/// Writes a path into a field the way the Browse button does, so whatever a
/// screen derives from a change - an alias, a working folder, a rescan - follows
/// a drop as readily as it follows a dialog.
export function setPath(input, path) {
  input.value = path;
  input.dispatchEvent(new Event('change'));
}

/// `key` names what the button picks rather than which screen holds it, so the
/// destination of either mode shares one remembered directory, and the backend
/// opens the dialog there instead of where another field's dialog last ended.
/// The button says what it does by shape: a folder is what every one of these
/// picks, and the word it used to wear only took the room the path needs.
export function withBrowseButton(input, command, key) {
  const row = document.createElement('div');
  row.className = 'row';

  const browse = iconButton(folder, 'form.browse', 'glyph');
  browse.addEventListener('click', async () => {
    const picked = await invoke(command, { key, path: input.value });
    if (picked) {
      setPath(input, picked);
    }
  });

  row.append(input, browse);
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

/// One cell of a table: text, or whatever element the row has to hold.
export function cell(content, className) {
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
export function pathCell(path) {
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

/// The names a result card lists - what was written, what was not.
export function list(items) {
  const element = document.createElement('ul');
  for (const item of items) {
    const entry = document.createElement('li');
    entry.textContent = item;
    element.append(entry);
  }
  return element;
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
/// `kind` names the card beyond the generic one, so a card that is a name plate
/// can be drawn narrower than one that holds a form.
///
/// `title` is the key the card is named by rather than its sentence, so a card
/// that stays open across a language change is still named in the new one.
export function modalDialog(kind) {
  const dialog = document.createElement('dialog');
  const card = document.createElement('div');
  card.className = kind ? `dialog-card ${kind}` : 'dialog-card';
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
    body.replaceChildren(content);
    actions.replaceChildren(...buttons);
    // The card's own name is bound to its key here rather than filled with a
    // sentence, so a language change reaches a card that is already open.
    setLabel(dialog, title);
    // The content is filled long after the screen's one translation pass, so its
    // own strings are localised on the way out.
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

/// A panel along the right edge of the window, for the choices that apply as
/// they are picked. A centred card dims the very thing the choice changes, so
/// the person judging a theme or a language has to close the card first to see
/// it; a sheet leaves that thing on screen the whole time.
///
/// It carries no close button of its own. `showModal()` answers Esc, the backdrop
/// stays clear so a click anywhere off the panel is the second way out, and those
/// are the two the system's own flyouts offer - which is also why there is
/// nothing to confirm here and no footer to put a button in.
export function sideSheet() {
  const dialog = document.createElement('dialog');
  dialog.className = 'sheet';
  const card = document.createElement('div');
  card.className = 'sheet-card';

  const head = document.createElement('header');
  head.className = 'sheet-head';
  const title = document.createElement('p');
  title.className = 'group';
  head.append(title);

  const body = document.createElement('div');
  body.className = 'dialog-body';

  card.append(head, body);
  dialog.append(card);
  dialog.addEventListener('click', (event) => {
    if (event.target === dialog) {
      dialog.close();
    }
  });

  dialog.openSheet = (titleKey, content) => {
    // Bound to the key rather than filled in, so a language change reaches a
    // sheet that is already open.
    setText(title, titleKey);
    body.replaceChildren(content);
    setLabel(dialog, titleKey);
    localize(dialog);
    // Left to the modal's own focusing rule, which lands on the first control of
    // the body: the panel is here to be changed, and its heading has nothing to
    // hold the focus.
    dialog.showModal();
  };
  document.body.append(dialog);
  return dialog;
}

/// A choice between a few values that all deserve to be seen at once. A cell
/// carries either a glyph or a word - `key` names it either way, as the radio's
/// accessible name and as the tooltip - because the words for a theme are not
/// the same length in two languages, and a row of cells that has to fit the
/// longer of the two is a row of mostly empty cells.
///
/// The dots stay in the markup and only lose their paint: a radio group is the
/// one control with arrow keys, and replacing it with buttons would take those
/// with it.
export function segmented(name, captionKey, options, picked, onPick) {
  const group = document.createElement('div');
  group.className = 'segmented';
  group.setAttribute('role', 'radiogroup');
  setLabel(group, captionKey);
  for (const option of options) {
    const input = document.createElement('input');
    input.type = 'radio';
    input.name = name;
    input.value = option.value;
    input.checked = option.value === picked();
    setLabel(input, option.key);
    input.addEventListener('change', () => onPick(option.value));

    const cell = document.createElement('label');
    cell.className = 'segment';
    setTitle(cell, option.key);
    if (option.glyph) {
      cell.textContent = option.glyph;
      cell.classList.add('mark');
    } else {
      // The key is left on the cell rather than resolved here, so a language
      // change reaches a panel that is already open.
      setText(cell, option.key);
    }
    // The dot goes inside its own cell, which is what makes the whole of the
    // cell clickable without a second handler.
    cell.prepend(input);
    group.append(cell);
  }
  return group;
}

/// A dropdown over values that are names rather than decisions. `options` carry
/// a key to translate or a name to show as it is, which is how a language can be
/// recognized in an app that does not speak it yet. The caller names the control
/// with a `field`, since a `select` is the one control a `<label>` can point at.
export function dropdown(options, picked, onPick) {
  const select = document.createElement('select');
  select.id = nextFieldId();
  for (const option of options) {
    const element = document.createElement('option');
    element.value = option.value;
    if (option.key) {
      setText(element, option.key);
    } else {
      element.textContent = option.name;
    }
    select.append(element);
  }
  select.value = picked();
  select.addEventListener('change', () => onPick(select.value));
  return select;
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
/// also comes with the ways to act on the folder it wrote into (6) - unless the
/// folder is already there, which the result asks the system about rather than
/// assume, because a reminder to do a thing that is done reads as a failure.
export function resultDialog() {
  const dialog = modalDialog();
  const card = dialog.querySelector('.dialog-card');

  function show(title, content, folder, onPath) {
    const actions = [];
    if (folder) {
      actions.push(openFolderButton(folder));
      if (!onPath) {
        actions.push(editPathButton());
      }
    }
    dialog.openModal(title, content, [...actions, closeButton(dialog)]);
  }

  dialog.success = async (folder, build) => {
    card.classList.remove('error');
    // The answer is the variable this window was started with, so an entry added
    // while it stayed open is not seen: the reminder is then repeated rather than
    // withheld, which is the safe direction to be wrong in.
    const onPath = folder && (await invoke('folder_on_path', { path: folder }).catch(() => false));
    const content = document.createElement('div');
    build(content, onPath);
    show('result.title', content, folder, onPath);
  };
  /// `key` is the error's own, so the card's title is the sentence the next
  /// language pass can rewrite; the detail beside it is what the backend said.
  dialog.fail = (key, detail) => {
    card.classList.add('error');
    const content = document.createElement('div');
    const text = t(key);
    content.textContent = detail ? `${text} ${detail}` : text;
    show(key, content);
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
