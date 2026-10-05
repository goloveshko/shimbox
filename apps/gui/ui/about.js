// What this build is: the version, the runners carried inside it, and where to
// read more. Shown in the same card the result of a run uses (docs/techspec.md 5).
import { closeButton, formatSize, invoke, modalDialog } from './form.js';
import { t } from './i18n.js';

const repository = 'https://github.com/goloveshko/shimbox';

function row(labelKey, value, extra) {
  const line = document.createElement('p');
  line.className = 'about-row';
  const label = document.createElement('span');
  label.dataset.i18n = labelKey;
  const answer = document.createElement('span');
  answer.className = extra ?? '';
  answer.textContent = value;
  line.append(label, answer);
  return line;
}

export function aboutDialog() {
  const dialog = modalDialog();

  async function show() {
    // Read on every look: the sizes are the built ones, and a stale card is worse
    // than two extra invokes.
    const [version, runners] = await Promise.all([
      window.__TAURI__.app.getVersion(),
      invoke('embedded_runners', {}),
    ]);

    const body = document.createElement('div');
    const head = document.createElement('p');
    head.className = 'about-head';
    const name = document.createElement('strong');
    name.textContent = t('app.title');
    const badge = document.createElement('span');
    badge.className = 'about-version';
    badge.textContent = version;
    head.append(name, badge);

    const tagline = document.createElement('p');
    tagline.className = 'about-tagline';
    tagline.textContent = t('about.tagline');

    body.append(
      head,
      tagline,
      row(
        'about.runners',
        runners.map((runner) => `${runner.variant} ${formatSize(runner.size)}`).join(' / ')
      ),
      row('about.license', 'MIT'),
      row('about.author', 'Sergey Goloveshko'),
      // A link that opens a browser would need a new way to leave the window, so
      // the address is the part that is selected whole by one click.
      row('about.repository', repository, 'about-url')
    );

    dialog.openModal(t('about.title'), body, [closeButton(dialog)]);
  }

  return { dialog, show };
}
