// Mode 1 of the spec (docs/techspec.md 5.1): one shim at a time.
import { setText, t } from '../i18n.js';
import { environmentPanel } from '../environment.js';
import {
  autofill,
  field,
  fieldError,
  folderOf,
  invoke,
  nameOf,
  nextFieldId,
  note,
  prefillFolder,
  resultDialog,
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
  const targetBlock = field(
    'single.target',
    target,
    withBrowseButton(target, 'pick_file', 'target')
  );
  fields.append(targetBlock);

  const destination = textInput('single.destination.placeholder');
  const destinationBlock = field(
    'single.destination',
    destination,
    withBrowseButton(destination, 'pick_folder', 'destination'),
    note('single.destination.help')
  );
  fields.append(destinationBlock);
  prefillFolder(destination, 'destination');

  const alias = textInput('single.alias.placeholder');
  const aliasBlock = field('single.alias', alias, alias);
  fields.append(aliasBlock);

  const args = textInput('single.arguments.placeholder');
  const argsBlock = field('single.arguments', args, args, note('single.arguments.help'));
  fields.append(argsBlock);

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
  const workdirBlock = field(
    'single.workdir',
    workdir,
    withBrowseButton(workdir, 'pick_folder', 'workdir'),
    workdirNote
  );
  fields.append(workdirBlock);

  const result = resultDialog();

  // The variables belong to the target rather than to the pair on `PATH`, and a
  // secret among them is the one value the config must not carry (3.3, 3.5).
  const environment = environmentPanel(result.fail);
  fields.append(environment.block);

  /// The lines a refusal can be about: the backend names the one that caused it,
  /// and the reason is written under it rather than over the form the person is
  /// filling in (5.1). An environment row answers through the panel, since which
  /// of them is meant is said by the variable it holds.
  const errors = {
    target: fieldError(targetBlock, target),
    destination: fieldError(destinationBlock, destination),
    alias: fieldError(aliasBlock, alias),
    arguments: fieldError(argsBlock, args),
    workdir: fieldError(workdirBlock, workdir),
  };

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
    generate.disabled = !target.value.trim() || !destination.value.trim() || !alias.value.trim();
  }

  for (const input of [target, destination, alias]) {
    for (const event of ['input', 'change']) {
      input.addEventListener(event, checkComplete);
    }
  }

  const fillAlias = autofill(alias, () => stemOf(target.value));

  /// The field is the only thing that decides where the child runs: empty leaves
  /// it in the caller's folder, a path pins one (3.4).
  function showWorkdirNote() {
    setText(workdirNote, workdir.value.trim() ? 'single.workdir.custom' : 'single.workdir.help');
  }

  target.addEventListener('change', async () => {
    const path = target.value.trim();
    fillAlias();
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
    workdir.addEventListener(event, showWorkdirNote);
  }
  showWorkdirNote();

  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    // A reason under one line says nothing about this attempt, and a row that was
    // refused is fixed by typing in it, which clears its own line but not another's.
    for (const error of Object.values(errors)) {
      error.clear();
    }
    environment.clearErrors();
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
      result.success(folder, (box, onPath) => {
        box.append(
          t('single.result', {
            name: nameOf(created.exe),
            destination: folder,
          }),
          ' ',
          t('single.result.variant', { variant: t(`variant.${created.variant}`) }),
          document.createElement('br'),
          t(onPath ? 'result.path.on' : 'result.path')
        );
      });
    } catch (failure) {
      report(failure);
    } finally {
      checkComplete();
    }
  });

  /// A refusal that belongs to one line of this form is answered under that line,
  /// because the thing to fix is what a person is looking at and a card would
  /// cover it. What has no line of the form — a folder that is gone, a pair that
  /// could not be written — keeps the card, which is also where the actions on a
  /// result live (5.1).
  function report(failure) {
    const key = `error.${failure.code}`;
    if (failure.field === 'env' && environment.flag(failure.name, key, failure.detail)) {
      return;
    }
    const where = errors[failure.field];
    if (where) {
      where.show(key, failure.detail);
    } else {
      result.fail(key, failure.detail);
    }
  }

  // The fields a drop can aim at, named by what they take rather than by the
  // screen that holds them, which is what ui/drop.js routes a path by.
  return { target, destination, workdir };
}
