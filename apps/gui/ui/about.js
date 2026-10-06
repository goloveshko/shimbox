// What this build is: the version, the runners carried inside it, and where to
// read more. Shown in the same card the result of a run uses (docs/techspec.md 5).
import { closeButton, folder, formatSize, iconButton, invoke, modalDialog } from './form.js';
import { t } from './i18n.js';

const repository = 'https://github.com/goloveshko/shimbox';
const license = `${repository}/blob/main/LICENSE`;
const homepage = 'https://sergey.is-a.dev';

function line(labelKey, answer) {
  const row = document.createElement('p');
  row.className = 'about-row';
  const label = document.createElement('span');
  label.dataset.i18n = labelKey;
  row.append(label, answer);
  return row;
}

function textRow(labelKey, value) {
  const answer = document.createElement('span');
  answer.textContent = value;
  return line(labelKey, answer);
}

/// `href` makes the answer the page it names. The click is handed to the backend
/// rather than followed: a webview that navigates away leaves the app behind the
/// page, and the card would have to be reopened to get back. Where the words are
/// not the address, the tooltip says where the link goes.
function linkRow(labelKey, text, href) {
  const link = document.createElement('a');
  link.href = href;
  link.textContent = text;
  if (text !== href) {
    link.title = href;
  }
  link.addEventListener('click', (event) => {
    event.preventDefault();
    invoke('open_link', { url: href });
  });
  return line(labelKey, link);
}

/// The path is the long part and opening it is the reason for the row, so the
/// button keeps the end of the line and the path loses its middle to an ellipsis.
function folderRow(path) {
  const cell = document.createElement('span');
  cell.className = 'about-folder';
  const shown = document.createElement('span');
  shown.className = 'about-path';
  shown.textContent = path;
  // The ellipsis is what hides the path, so the whole of it goes to the tooltip.
  shown.title = path;
  const open = iconButton(folder, 'about.folder.open', 'glyph');
  open.addEventListener('click', () => {
    invoke('open_folder', { path });
  });
  cell.append(shown, open);
  return line('about.folder', cell);
}

export function aboutDialog() {
  const dialog = modalDialog();

  async function show() {
    // Read on every look: the sizes are the built ones, and a stale card is worse
    // than two extra invokes.
    const [version, runners, configDir] = await Promise.all([
      window.__TAURI__.app.getVersion(),
      invoke('embedded_runners', {}),
      invoke('app_folder', {}),
    ]);

    const body = document.createElement('div');
    const head = document.createElement('p');
    head.className = 'about-head';
    const name = document.createElement('strong');
    name.textContent = t('app.title');
    // The number answers "which build is this" far more often than it is read, so
    // the badge is the button that copies it.
    const badge = document.createElement('button');
    badge.type = 'button';
    badge.className = 'about-version';
    badge.textContent = version;
    badge.title = t('about.version.copy');
    badge.addEventListener('click', () => {
      // A write the webview refuses says nothing at all here: the badge claims
      // only what actually landed, and a click is what the clipboard asks for.
      navigator.clipboard.writeText(version).then(() => {
        badge.textContent = t('about.version.copied');
        setTimeout(() => {
          badge.textContent = version;
        }, 1200);
      }, () => {});
    });
    head.append(name, badge);

    const tagline = document.createElement('p');
    tagline.className = 'about-tagline';
    tagline.textContent = t('about.tagline');

    body.append(
      head,
      tagline,
      textRow(
        'about.runners',
        runners.map((runner) => `${runner.variant} ${formatSize(runner.size)}`).join(' / ')
      ),
      linkRow('about.license', 'MIT', license),
      linkRow('about.author', 'Sergey Goloveshko', homepage),
      linkRow('about.repository', repository, repository),
      // A folder the app cannot name is a row with nothing to open in it.
      ...(configDir ? [folderRow(configDir)] : [])
    );

    dialog.openModal(t('about.title'), body, [closeButton(dialog)]);
  }

  return { dialog, show };
}
