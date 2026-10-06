// The store a `cred` row points at (docs/techspec.md 3.5): the names Windows
// keeps, with the way to add one and to forget one. It is a card rather than a
// block of the form because a token is typed once per secret and read once per
// shim, so the rare half of the job should not hold the screen the frequent half
// works on.
import { button, closeButton, iconButton, invoke, modalDialog, note, textInput } from './form.js';
import { t } from './i18n.js';

/// Segoe's own trash, so the one action that destroys something does not wear the
/// `−` a row uses for the same-looking but harmless removal.
const trash = '\uE74D';

/// A secret is typed once and read by nobody over the shoulder, so it is masked;
/// what the runner receives is the text, not the dots.
function secretInput(placeholderKey) {
  const input = document.createElement('input');
  input.type = 'password';
  input.autocomplete = 'off';
  input.spellcheck = false;
  input.dataset.i18nPlaceholder = placeholderKey;
  return input;
}

/// `onChanged` takes the fresh list and, when the card is what stored it, the name
/// that was just added. `report` takes a failure the store has no line of its own
/// to sit on, such as a credential Windows refused.
export function secretsDialog({ onChanged, report }) {
  const dialog = modalDialog();

  // The card has a caption of its own because it is reached from two places and
  // has to say which one it is even when the form behind it is dimmed.
  const title = document.createElement('p');
  title.className = 'group';

  const list = document.createElement('ul');
  const none = note('env.secret.empty');

  const name = textInput('env.secret.name.placeholder');
  name.setAttribute('aria-label', t('env.secret.name'));
  const value = secretInput('env.secret.value.placeholder');
  value.setAttribute('aria-label', t('env.secret.value'));
  const put = button('env.secret.store');

  function entry(target) {
    const line = document.createElement('li');
    const label = document.createElement('span');
    label.textContent = target;
    const drop = iconButton(trash, 'env.secret.forget', 'glyph trash');
    drop.addEventListener('click', async () => {
      try {
        await invoke('forget_secret', { target });
        await refresh(null);
      } catch (failure) {
        report(t(`error.${failure.code}`), failure.detail);
      }
    });
    // The name is what is read and the action what is aimed at, so the name keeps
    // the left edge of the line and the icon takes its end.
    line.append(label, drop);
    return line;
  }

  async function refresh(created) {
    const secrets = await invoke('list_secrets', {});
    list.replaceChildren(...secrets.map(entry));
    list.hidden = secrets.length === 0;
    none.hidden = secrets.length > 0;
    // The caption counts what is stored, so it is rewritten wherever the count
    // changes, including while the card is already open.
    const caption = t('env.secret.title', { count: secrets.length });
    title.textContent = caption;
    dialog.setAttribute('aria-label', caption);
    // The rows' pickers list these same names, and the point of a named secret is
    // that rows share it, so all of them hear about a change at once.
    onChanged(secrets, created);
  }

  put.addEventListener('click', async () => {
    put.disabled = true;
    try {
      // The command answers with the name a config refers to, which is the same
      // string the rows list, so a stored secret can be handed back as a choice.
      const stored = await invoke('store_secret', {
        name: name.value,
        value: value.value,
      });
      // Both fields are cleared: the typed token stays in the box only until it is
      // stored, and a name left behind would be stored twice by accident.
      name.value = '';
      value.value = '';
      await refresh(stored);
    } catch (failure) {
      report(t(`error.${failure.code}`), failure.detail);
    } finally {
      put.disabled = false;
    }
  });

  const row = document.createElement('div');
  row.className = 'row';
  row.append(name, value, put);

  const body = document.createElement('div');
  body.className = 'secret-store';
  body.append(title, none, list, row, note('env.secret.help'));

  async function show() {
    // Read on every look, so the card opens on what is stored now rather than on
    // what was stored when the window was built.
    await refresh(null);
    dialog.openModal(title.textContent, body, [closeButton(dialog)]);
  }

  return { dialog, show };
}
