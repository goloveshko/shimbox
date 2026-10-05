// The child environment of mode 1 (docs/techspec.md 3.3, 3.5): one row per
// variable, and the `cred` rows that point at a named secret. Which config key a
// kind turns into is the backend's call, so nothing here parses or validates.
import { button, invoke, note, textInput, withBrowseButton } from './form.js';
import { secretsDialog } from './secrets.js';
import { t, localize } from './i18n.js';

const kinds = ['value', 'cred', 'file'];

/// The picker's own way out of an empty store, and never a name Windows could
/// hold, so it cannot collide with a target.
const create = '+';

/// `report` takes a failure that has no field of its own to sit on, such as a
/// secret the store refused.
export function environmentPanel(report) {
  const lines = document.createElement('div');
  lines.className = 'env-rows';

  /// Every `cred` picker, so storing or forgetting one name updates all of
  /// them at once: the point of a named secret is that rows share it.
  const picks = new Set();
  let secrets = [];

  /// The picker that opened the card, so a name stored there lands in the row
  /// that went looking for it.
  let asked = null;

  const card = secretsDialog({
    onChanged: (next, created) => {
      const who = asked;
      paint(next);
      if (created && who && next.includes(created)) {
        who.value = created;
      }
    },
    report,
  });

  card.dialog.addEventListener('close', () => {
    // A card closed without storing anything leaves its picker on the option that
    // opened it, which is not a value a row may send.
    if (asked?.value === create) {
      asked.value = '';
    }
    asked = null;
  });

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
    // Last, so a long store does not push the way to add one off the list.
    const fresh = document.createElement('option');
    fresh.value = create;
    fresh.textContent = t('env.secret.new');
    pick.append(fresh);
    // A name forgotten elsewhere in the session is not offered again, and a
    // picker left pointing at it would send a reference that cannot resolve.
    pick.value = secrets.includes(chosen) ? chosen : '';
  }

  function paint(next) {
    secrets = next;
    for (const pick of picks) {
      fill(pick);
    }
    opener.textContent = t('env.secret.title', { count: next.length });
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
    const browse = withBrowseButton(path, 'pick_file', 'env.file');
    browse.hidden = true;

    const pick = document.createElement('select');
    pick.setAttribute('aria-label', t('env.kind.cred'));
    pick.className = 'pick';
    picks.add(pick);
    fill(pick);
    // The store is a card, not a part of this screen, so the option that says
    // "none of these, make one" is what opens it.
    pick.addEventListener('change', () => {
      if (pick.value !== create) {
        return;
      }
      asked = pick;
      card.show();
    });

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

  // The store's caption counts what is in it, so it is written rather than keyed:
  // a translation pass would put the `{count}` back on screen.
  const opener = document.createElement('button');
  opener.type = 'button';
  opener.className = 'small';
  opener.textContent = t('env.secret.title', { count: 0 });
  opener.addEventListener('click', () => card.show());

  const actions = document.createElement('div');
  actions.className = 'row env-actions';
  actions.append(add, opener);

  const title = document.createElement('p');
  title.className = 'group';
  title.dataset.i18n = 'env.title';

  const block = document.createElement('div');
  block.className = 'field span-all env';
  block.append(title, lines, actions, note('env.help'));

  invoke('list_secrets', {})
    .then(paint)
    .catch(() => {
      // An empty store is what the panel was built for, not a reason to complain.
    });

  return {
    block,
    values: () => [...lines.children].map((line) => line.value()),
  };
}
