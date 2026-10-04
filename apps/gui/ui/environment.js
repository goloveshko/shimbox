// The child environment of mode 1 (docs/techspec.md 3.3, 3.5): one row per
// variable, and the named secrets a `cred` row points at. Which config key a
// kind turns into is the backend's call, so nothing here parses or validates.
import { t, localize } from './i18n.js';
import { invoke, note, textInput, withBrowseButton } from './form.js';

const kinds = ['value', 'cred', 'file'];

function button(key, className) {
  const element = document.createElement('button');
  element.type = 'button';
  element.dataset.i18n = key;
  element.className = className ?? '';
  return element;
}

/// A secret is typed once and read by nobody over the shoulder, so it is
/// masked; what the runner receives is the text, not the dots.
function secretInput(placeholderKey) {
  const input = document.createElement('input');
  input.type = 'password';
  input.autocomplete = 'off';
  input.spellcheck = false;
  input.dataset.i18nPlaceholder = placeholderKey;
  return input;
}

/// `report` takes a failure that has no field of its own to sit on, such as a
/// secret the store refused.
export function environmentPanel(report) {
  const lines = document.createElement('div');
  lines.className = 'env-rows';

  /// Every `cred` picker, so storing or forgetting one name updates all of
  /// them at once: the point of a named secret is that rows share it.
  const picks = new Set();
  let secrets = [];

  function fill(pick) {
    const chosen = pick.value;
    pick.replaceChildren();
    const empty = document.createElement('option');
    empty.value = '';
    empty.textContent = secrets.length ? t('env.secret.pick') : t('env.secret.none');
    pick.append(empty);
    for (const target of secrets) {
      const option = document.createElement('option');
      option.value = target;
      option.textContent = target;
      pick.append(option);
    }
    // A name forgotten elsewhere in the session is not offered again, and a
    // picker left pointing at it would send a reference that cannot resolve.
    pick.value = secrets.includes(chosen) ? chosen : '';
  }

  async function reload() {
    secrets = await invoke('list_secrets', {});
    renderStore();
    for (const pick of picks) {
      fill(pick);
    }
  }

  function addLine() {
    const line = document.createElement('div');
    line.className = 'env-row';

    const name = textInput('env.name.placeholder');
    name.setAttribute('aria-label', t('env.name'));

    const kind = document.createElement('select');
    kind.setAttribute('aria-label', t('env.kind'));
    for (const value of kinds) {
      const option = document.createElement('option');
      option.value = value;
      option.textContent = t(`env.kind.${value}`);
      kind.append(option);
    }

    // One control per kind, kept alive while the row is, so switching back to a
    // kind does not lose what was typed into the one that was showing.
    const literal = textInput('env.value.placeholder');
    literal.setAttribute('aria-label', t('env.kind.value'));

    const path = textInput('env.file.placeholder');
    path.setAttribute('aria-label', t('env.kind.file'));
    const browse = withBrowseButton(path, 'pick_file');
    browse.hidden = true;

    const pick = document.createElement('select');
    pick.setAttribute('aria-label', t('env.kind.cred'));
    pick.className = 'pick';
    picks.add(pick);
    fill(pick);

    const remove = button('env.remove', 'small');
    remove.setAttribute('aria-label', t('env.remove'));
    remove.addEventListener('click', () => {
      picks.delete(pick);
      line.remove();
    });

    function show() {
      const value = kind.value;
      literal.hidden = value !== 'value';
      browse.hidden = value !== 'file';
      pick.hidden = value !== 'cred';
    }

    kind.addEventListener('change', show);
    show();

    line.append(name, kind, literal, browse, pick, remove);
    /// The value the row holds now: the kind says which of its controls counts,
    /// and an empty name is the backend's cue to write nothing at all.
    line.value = () => ({
      name: name.value,
      kind: kind.value,
      value: { value: literal, file: path, cred: pick }[kind.value].value,
    });

    lines.append(line);
    // The screen's one `localize()` pass happened before this row existed.
    localize(line);
    name.focus();
  }

  const add = button('env.add', 'small');
  add.addEventListener('click', () => addLine());

  // The store: the same names the pickers list, with a way to add and to remove
  // one. It is kept behind a disclosure because it is the part a user touches
  // once per token rather than once per shim.
  const stored = document.createElement('ul');
  const summary = document.createElement('summary');
  // The count is the summary's whole text, so it is filled in before the store
  // has answered rather than left blank until it does.
  summary.textContent = t('env.secret.title', { count: 0 });
  const secretName = textInput('env.secret.name.placeholder');
  secretName.setAttribute('aria-label', t('env.secret.name'));
  const secretValue = secretInput('env.secret.value.placeholder');
  secretValue.setAttribute('aria-label', t('env.secret.value'));
  const put = button('env.secret.store', 'small');

  function renderStore() {
    stored.replaceChildren();
    for (const target of secrets) {
      const entry = document.createElement('li');
      const label = document.createElement('span');
      label.textContent = target;
      const drop = button('env.secret.forget', 'small');
      drop.addEventListener('click', async () => {
        try {
          await invoke('forget_secret', { target });
          await reload();
        } catch (failure) {
          report(t(`error.${failure.code}`), failure.detail);
        }
      });
      entry.append(label, drop);
      stored.append(entry);
    }
    // The Forget buttons are rebuilt here, after the screen's one translation
    // pass, so their captions have to be filled in on the way out.
    localize(stored);
    summary.textContent = t('env.secret.title', { count: secrets.length });
  }

  put.addEventListener('click', async () => {
    put.disabled = true;
    try {
      await invoke('store_secret', {
        name: secretName.value,
        value: secretValue.value,
      });
      // Both fields are cleared: the typed token stays in the box only until it
      // is stored, and a name left behind would be stored twice by accident.
      secretName.value = '';
      secretValue.value = '';
      await reload();
    } catch (failure) {
      report(t(`error.${failure.code}`), failure.detail);
    } finally {
      put.disabled = false;
    }
  });

  const entry = document.createElement('div');
  entry.className = 'row';
  entry.append(secretName, secretValue, put);

  const details = document.createElement('details');
  details.className = 'secrets';
  details.append(summary, stored, entry, note('env.secret.help'));

  const title = document.createElement('p');
  title.className = 'group';
  title.dataset.i18n = 'env.title';

  const block = document.createElement('div');
  block.className = 'field span-all env';
  block.append(title, lines, add, note('env.help'), details);

  reload().catch(() => {
    // An empty store is what the panel was built for, not a reason to complain.
  });

  return {
    block,
    values: () => [...lines.children].map((line) => line.value()),
  };
}
