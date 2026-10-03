// One file per language, registered in i18n.js. Keys are grouped by screen.
export default {
  'app.title': 'shimbox',

  'tab.single': 'Single shim',

  'status.runners': 'embedded runner: {size}',

  'single.target': 'Target executable',
  'single.target.placeholder': 'C:\\Tools\\ffmpeg\\bin\\ffmpeg.exe',
  'single.browse': 'Browse',
  'single.destination': 'Destination folder',
  'single.destination.placeholder': 'C:\\Shims',
  'single.destination.help': 'Where the .exe and .shim pair is written.',
  'single.alias': 'Shim name',
  'single.alias.placeholder': 'ffmpeg',
  'single.arguments': 'Default arguments',
  'single.arguments.placeholder': '-hide_banner -loglevel error',
  'single.arguments.help': 'Inserted before the caller arguments, verbatim.',
  'single.variant': 'Runner type',
  'single.variant.auto': 'Auto detect',
  'single.variant.cli': 'CLI (Console)',
  'single.variant.gui': 'GUI (Detached)',
  'single.detected.gui': 'Detected: GUI application.',
  'single.detected.console': 'Detected: console application.',
  'single.detected.unknown':
    'Detected: not a readable PE, the console runner will be used.',
  'single.workdir': 'Working directory',
  'single.workdir.placeholder': 'C:\\Tools\\ffmpeg',
  'single.workdir.help': 'Leave empty to run the target from its own folder.',
  'single.workdir.default': 'Runs from {folder} by default.',

  'single.generate': 'Generate shim',
  'single.result': '{name} is ready in {destination}.',
  'single.result.variant': 'Runner type: {variant}.',
  'single.result.path':
    'Add the destination folder to PATH to start the shim by name.',
  'single.result.open': 'Open destination folder',

  'error.target': 'The target executable does not exist.',
  'error.destination': 'The destination folder does not exist.',
  'error.alias': 'The shim name is not a valid file name.',
  'error.collision': 'A file with that name is already there.',
  'error.write': 'The shim could not be written.',
};
