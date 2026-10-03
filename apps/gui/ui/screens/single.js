// Mode 1 of the spec (docs/techspec.md 5.1): one shim at a time.
import { t } from '../i18n.js';
import {
  autofill,
  folderOf,
  invoke,
  labelFor,
  messagePanel,
  nextFieldId,
  note,
  openFolderButton,
  stemOf,
  textInput,
  withBrowseButton,
} from '../form.js';

export function renderSingle(root) {
  const form = document.createElement('form');

  const target = textInput('single.target.placeholder');
  form.append(labelFor('single.target', target), withBrowseButton(target, 'pick_file'));

  const destination = textInput('single.destination.placeholder');
  form.append(
    labelFor('single.destination', destination),
    withBrowseButton(destination, 'pick_folder'),
    note('single.destination.help')
  );

  const alias = textInput('single.alias.placeholder');
  form.append(labelFor('single.alias', alias), alias);

  const args = textInput('single.arguments.placeholder');
  form.append(labelFor('single.arguments', args), args, note('single.arguments.help'));

  const variant = document.createElement('select');
  variant.id = nextFieldId();
  for (const value of ['auto', 'cli', 'gui']) {
    const option = document.createElement('option');
    option.value = value;
    option.dataset.i18n = `variant.${value}`;
    variant.append(option);
  }
  const detected = note();
  form.append(labelFor('single.variant', variant), variant, detected);

  const workdir = textInput('single.workdir.placeholder');
  const workdirNote = note();
  form.append(
    labelFor('single.workdir', workdir),
    withBrowseButton(workdir, 'pick_folder'),
    workdirNote
  );

  const generate = document.createElement('button');
  generate.type = 'submit';
  generate.dataset.i18n = 'single.generate';
  form.append(generate);

  const result = messagePanel();
  root.append(form, result);

  const fillAlias = autofill(alias, () => stemOf(target.value));
  const fillDestination = autofill(destination, () => folderOf(target.value));

  // The default working directory belongs to the runner, not to the config:
  // naming the folder here keeps it visible without pinning it into every
  // shim, which would stop following a target that moves (3.4).
  function showWorkdirDefault() {
    const folder = workdir.value.trim() ? '' : folderOf(target.value);
    workdirNote.textContent = folder
      ? t('single.workdir.default', { folder })
      : t('single.workdir.help');
  }

  target.addEventListener('change', async () => {
    const path = target.value.trim();
    fillAlias();
    fillDestination();
    showWorkdirDefault();
    result.clear();
    if (!path) {
      detected.textContent = '';
      return;
    }
    const found = await invoke('detect_variant', { path });
    detected.textContent = t(`single.detected.${found.subsystem}`);
  });

  workdir.addEventListener('input', showWorkdirDefault);
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
