// One file per language, registered in i18n.js. Keys are grouped by screen.
export default {
  'app.title': 'shimbox',

  'tab.single': 'Single shim',

  'status.runners': 'embedded runner: {size}',

  'single.target': 'Target executable',
  'single.target.placeholder': 'C:\\Tools\\ffmpeg\\bin\\ffmpeg.exe',
  'single.browse': 'Browse',
  'single.alias': 'Shim name',
  'single.alias.placeholder': 'ffmpeg',
  'single.arguments': 'Default arguments',
  'single.arguments.placeholder': '-hide_banner -loglevel error',
  'single.arguments.help': 'Inserted before the caller arguments, verbatim.',
  'single.variant': 'Runner type',
  'single.variant.auto': 'Auto detect',
  'single.variant.cli': 'CLI (Console)',
  'single.variant.gui': 'GUI (Detached)',
  'single.variant.detected': 'Detected: {subsystem} application.',
  'single.variant.unknown':
    'Detected: not a readable PE, the console runner will be used.',
  'single.workdir': 'Working directory',
  'single.workdir.placeholder': 'defaults to the target folder',
  'single.workdir.help': 'Leave empty to run the target from its own folder.',
};
