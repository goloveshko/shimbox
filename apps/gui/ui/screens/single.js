// Mode 1 of the spec (docs/techspec.md 5.1): one shim at a time.
import { t } from '../i18n.js';
import {
  autofill,
  field,
  folderOf,
  invoke,
  messagePanel,
  nextFieldId,
  note,
  openFolderButton,
  sameFolder,
  stemOf,
  textInput,
  withBrowseButton,
} from '../form.js';

export function renderSingle(root) {
  const form = document.createElement('form');

  const target = textInput('single.target.placeholder');
  form.append(field('single.target', target, withBrowseButton(target, 'pick_file')));

  const destination = textInput('single.destination.placeholder');
  form.append(
    field(
      'single.destination',
      destination,
      withBrowseButton(destination, 'pick_folder'),
      note('single.destination.help')
    )
  );

  const alias = textInput('single.alias.placeholder');
  form.append(field('single.alias', alias, alias));

  const args = textInput('single.arguments.placeholder');
  form.append(
    field('single.arguments', args, args, note('single.arguments.help'))
  );

  const variant = document.createElement('select');
  variant.id = nextFieldId();
  for (const value of ['auto', 'cli', 'gui']) {
    const option = document.createElement('option');
    option.value = value;
    option.dataset.i18n = `variant.${value}`;
    variant.append(option);
  }
  const detected = note();
  form.append(field('single.variant', variant, variant, detected));

  const workdir = textInput('single.workdir.placeholder');
  const workdirNote = note();
  form.append(
    field(
      'single.workdir',
      workdir,
      withBrowseButton(workdir, 'pick_folder'),
      workdirNote
    )
  );

  const generate = document.createElement('button');
  generate.type = 'submit';
  generate.dataset.i18n = 'single.generate';
  form.append(generate);

  const result = messagePanel();
  root.append(form, result);

  const fillAlias = autofill(alias, () => stemOf(target.value));
  // The target's folder is where the target runs, not where the pair belongs:
  // the destination has to be on `PATH` (6), and a pair written next to the
  // binary it shims would collide with the name the scan already has there.
  const fillWorkdir = autofill(workdir, () => folderOf(target.value));

  /// The prefilled folder is exactly what the runner falls back to, so the line
  /// under the field says whether the config will name one (3.4).
  function showWorkdirDefault() {
    const folder = folderOf(target.value);
    const value = workdir.value.trim();
    if (!folder && !value) {
      workdirNote.textContent = t('single.workdir.help');
    } else if (!value || sameFolder(value, folder)) {
      workdirNote.textContent = t('single.workdir.default', { folder });
    } else {
      workdirNote.textContent = t('single.workdir.custom');
    }
  }

  target.addEventListener('change', async () => {
    const path = target.value.trim();
    fillAlias();
    fillWorkdir();
    showWorkdirDefault();
    result.clear();
    if (!path) {
      detected.textContent = '';
      return;
    }
    const found = await invoke('detect_variant', { path });
    detected.textContent = t(`single.detected.${found.subsystem}`);
  });

  for (const event of ['input', 'change']) {
    workdir.addEventListener(event, showWorkdirDefault);
  }
  showWorkdirDefault();

  for (const input of [destination, alias, args, variant, workdir]) {
    input.addEventListener('input', () => result.clear());
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
      const folder = folderOf(created.exe);
      result.show((panel) => {
        panel.append(
          t('single.result', {
            name: created.exe.slice(folder.length + 1),
            destination: folder,
          }),
          ' ',
          t('single.result.variant', { variant: t(`variant.${created.variant}`) }),
          document.createElement('br'),
          t('result.path'),
          document.createElement('br'),
          openFolderButton(folder)
        );
      });
    } catch (failure) {
      result.fail(t(`error.${failure.code}`), failure.detail);
    } finally {
      generate.disabled = false;
    }
  });
}
