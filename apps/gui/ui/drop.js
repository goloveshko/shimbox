// What a path dropped onto the window means. It is the path the form was about to
// be asked for, so a drop aimed at a field fills that field, and a drop aimed at
// nothing in particular says which mode it is about instead: one program makes one
// shim, a folder or a handful of files is what the batch screen scans.
//
// Tauri takes the browser's own drag events away from the page and answers with an
// event of its own, which is why the pointer showed a copy cursor over a form that
// never moved. The position that event carries is the window's client area in
// physical pixels - wry turns the screen point into it, and this window has no
// caption to subtract - so the device pixel ratio is the whole mapping.
import { folderOf, invoke, setPath } from './form.js';

const executable = (path) => /\.exe$/i.test(path);

/// The fields a drop can be aimed at, from both screens at once: only the visible
/// screen's can end up under the pointer, so one list answers for either mode.
export const zones = (screens) => [
  { screen: 'single', key: 'target', kind: 'file', element: screens.single.target },
  { screen: 'single', key: 'destination', kind: 'folder', element: screens.single.destination },
  { screen: 'single', key: 'workdir', kind: 'folder', element: screens.single.workdir },
  { screen: 'batch', key: 'source', kind: 'folder', element: screens.batch.source },
  { screen: 'batch', key: 'destination', kind: 'folder', element: screens.batch.destination },
];

/// The deepest folder every dropped item lives in, which is the one a scan of the
/// dropped files has to read. A pair of drives sharing no folder is not a folder,
/// so the first item's own is used and the rest are simply not in the table.
export function commonFolder(paths, folders) {
  // `C:` on its own names whatever the current folder of that drive happens to be,
  // so a bare drive gets its separator back before it is used as a folder.
  const own = paths.map((path, index) => {
    const dir = folders[index] ? path : folderOf(path);
    return /^[a-z]:$/i.test(dir) ? `${dir}\\` : dir;
  });
  const parts = own.map((dir) => dir.replace(/\//g, '\\').replace(/\\+$/, '').split('\\'));
  const first = parts[0] ?? [];
  let shared = 0;
  while (
    shared < first.length &&
    parts.every((path) => (path[shared] ?? '').toLowerCase() === first[shared].toLowerCase())
  ) {
    shared += 1;
  }
  if (!shared) {
    return own[0] ?? '';
  }
  return `${first.slice(0, shared).join('\\')}${shared === 1 ? '\\' : ''}`;
}

/// Which field ends up holding what. `zone` is the field the pointer was over, if
/// any, and `folders[i]` says whether `paths[i]` is a folder. A file aimed at a
/// folder field is that file's own folder, and aimed at the folder to scan it is
/// also the choice the table should start with.
export function plan(paths, folders, zone) {
  const files = paths.filter((_, index) => !folders[index]);
  const dirs = paths.filter((_, index) => folders[index]);

  if (zone?.kind === 'file' && files.length) {
    return { screen: zone.screen, key: zone.key, path: files[0] };
  }
  // A folder aimed at the field that runs a program is not that field's answer, so
  // the drop is read as the batch it describes rather than as a target nobody can
  // start; the other direction works, because a file names the folder it is in.
  if (zone?.kind === 'folder' && dirs.length) {
    return { screen: zone.screen, key: zone.key, path: dirs[0] };
  }
  if (zone && files.length) {
    const path = folderOf(files[0]);
    return zone.key === 'source'
      ? { screen: 'batch', key: 'source', path, select: files }
      : { screen: zone.screen, key: zone.key, path };
  }

  if (paths.length === 1 && dirs.length === 1) {
    return { screen: 'batch', key: 'source', path: dirs[0] };
  }
  if (paths.length === 1 && executable(files[0])) {
    return { screen: 'single', key: 'target', path: files[0] };
  }
  return { screen: 'batch', key: 'source', path: commonFolder(paths, folders), select: files };
}

/// A message card belongs to the question it answers, so a drop while one is open
/// is a drop aimed at nothing and is left alone rather than rewriting the form
/// behind it.
const cardIsOpen = () => Boolean(document.querySelector('dialog[open]'));

function zoneUnder(position, list) {
  const ratio = window.devicePixelRatio || 1;
  const hit = document.elementFromPoint(position.x / ratio, position.y / ratio);
  const field = hit?.closest('.field');
  return field ? (list.find((zone) => field.contains(zone.element)) ?? null) : null;
}

/// Draws the drop answer for the window. `show` switches the mode, which is the
/// part a drop onto an empty patch of the form decides on its own.
export function dropHandler({ screens, show }) {
  const list = zones(screens);
  let marked = null;
  let held = null;
  let asked = { paths: null, folders: [] };

  function clear() {
    marked?.classList.remove('drop-on');
    marked = null;
  }

  /// The folders behind a dropped set of paths, once per drag: the pointer can
  /// cross half a dozen fields before it lets go, and the same names answer every
  /// one of them.
  async function kinds(paths) {
    if (asked.paths !== paths) {
      asked = {
        paths,
        folders: await invoke('paths_are_folders', { paths }).catch(() => paths.map(() => false)),
      };
    }
    return asked.folders;
  }

  function apply(action) {
    show(action.screen);
    if (action.screen === 'batch' && action.key === 'source') {
      return screens.batch.dropSource(action.path, action.select ?? []);
    }
    const zone = list.find((each) => each.screen === action.screen && each.key === action.key);
    setPath(zone.element, action.path);
    return undefined;
  }

  /// Whatever the pointer is over is only worth naming as the target when the plan
  /// really lands there; over a patch of the form the drop would rather not fill,
  /// what lights up is the mode it is about to switch to.
  function mark(zone, action) {
    const filled = zone && zone.screen === action.screen && zone.key === action.key;
    const box = filled
      ? zone.element.closest('.field')
      : document.getElementById(`tab-${action.screen}`);
    if (box === marked) {
      return;
    }
    clear();
    box?.classList.add('drop-on');
    marked = box;
  }

  return async function handle(event) {
    if (event.type === 'leave') {
      held = null;
      return clear();
    }
    // The names come with the drag itself and are not repeated by the moves that
    // follow it, so what `enter` was given is what every `over` is read against -
    // otherwise the mark drawn on entering is gone before the pointer stops.
    if (event.paths) {
      held = event.paths;
    }
    const paths = event.paths ?? held ?? [];
    if (!paths.length || cardIsOpen()) {
      return clear();
    }
    const zone = zoneUnder(event.position, list);
    const action = plan(paths, await kinds(paths), zone);
    if (event.type !== 'drop') {
      return mark(zone, action);
    }
    held = null;
    clear();
    apply(action);
  };
}
