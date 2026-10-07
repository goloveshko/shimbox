// Mode 1 of the spec (docs/techspec.md 5.1): one shim at a time.
import { setText, t } from '../i18n.js';
import { environmentPanel } from '../environment.js';
import {
  autofill,
  field,
  folderOf,
  invoke,
  nextFieldId,
  note,
  resultDialog,
  sameFolder,
  stemOf,
  textInput,
  withBrowseButton,
} from '../form.js';

export function renderSingle(root) {
  const form = document.createElement('form');
  // The fields are a block of their own so the card can stretch to the window and
  // only they scroll inside it, leaving the button on the bottom edge.
  const fields = document.createElement('div');
  fields.className = 'fields';
  form.append(fields);

  const target = textInput('single.target.placeholder');
  fields.append(field('single.target', target, withBrowseButton(target, 'pick_file', 'target')));

  const destination = textInput('single.destination.placeholder');
  fields.append(
    field(
      'single.destination',
      destination,
      withBrowseButton(destination, 'pick_folder', 'destination'),
      note('single.destination.help')
    )
  );

  const alias = textInput('single.alias.placeholder');
  fields.append(field('single.alias', alias, alias));

  const args = textInput('single.arguments.placeholder');
  fields.append(
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
  fields.append(field('single.variant', variant, variant, detected));

  const workdir = textInput('single.workdir.placeholder');
  const workdirNote = note();
  fields.append(
    field(
      'single.workdir',
      workdir,
      withBrowseButton(workdir, 'pick_folder', 'workdir'),
      workdirNote
    )
  );

  const result = resultDialog();

  // The variables belong to the target rather than to the pair on `PATH`, and a
  // secret among them is the one value the config must not carry (3.3, 3.5).
  const environment = environmentPanel(result.fail);
  fields.append(environment.block);

  const generate = document.createElement('button');
  generate.type = 'submit';
  generate.dataset.i18n = 'single.generate';
  generate.disabled = true;
  form.append(generate);

  root.append(form);

  /// A shim needs a target to run, a folder on `PATH` to be found in and the name
  /// it is found by, so the button waits for those three instead of answering an
  /// incomplete form with an error (5.1).
  function checkComplete() {
    generate.disabled =
      !target.value.trim() || !destination.value.trim() || !alias.value.trim();
  }

  for (const input of [target, destination, alias]) {
    for (const event of ['input', 'change']) {
      input.addEventListener(event, checkComplete);
    }
  }

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
      setText(workdirNote, 'single.workdir.help');
    } else if (!value || sameFolder(value, folder)) {
      setText(workdirNote, 'single.workdir.default', { folder });
    } else {
      setText(workdirNote, 'single.workdir.custom');
    }
  }

  target.addEventListener('change', async () => {
    const path = target.value.trim();
    fillAlias();
    fillWorkdir();
    showWorkdirDefault();
    // The alias arrived without an event of its own, so the button is told here.
    checkComplete();
    if (!path) {
      setText(detected, null);
      return;
    }
    const found = await invoke('detect_variant', { path });
    setText(detected, `single.detected.${found.subsystem}`);
  });

  for (const event of ['input', 'change']) {
    workdir.addEventListener(event, showWorkdirDefault);
  }
  showWorkdirDefault();

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
          environment: environment.values(),
        },
      });
      const folder = folderOf(created.exe);
      result.success(folder, (box) => {
        box.append(
          t('single.result', {
            name: created.exe.slice(folder.length + 1),
            destination: folder,
          }),
          ' ',
          t('single.result.variant', { variant: t(`variant.${created.variant}`) }),
          document.createElement('br'),
          t('result.path')
        );
      });
    } catch (failure) {
      result.fail(`error.${failure.code}`, failure.detail);
    } finally {
      checkComplete();
    }
  });
}
