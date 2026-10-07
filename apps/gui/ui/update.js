// The once-a-day look for a newer release (docs/techspec.md 5.6).
//
// The question is asked here rather than in Rust: the window already is a
// browser, so one public JSON file costs no HTTP client in the app. Rust holds
// only the moment of the ask and answers whether the version it names is ahead,
// which is the part worth testing.
import { closeButton, invoke, modalDialog } from './form.js';
import { setText } from './i18n.js';

const releases = 'https://api.github.com/repos/goloveshko/shimbox/releases/latest';

/// The release page is where the download is, and a browser shows a page better
/// than this card can. The address comes off the network, so it goes through
/// `open_link`, whose allow-list is what keeps a stranger's JSON from pointing
/// the person somewhere else.
function openButton(url) {
  const button = document.createElement('button');
  button.type = 'button';
  button.dataset.i18n = 'update.open';
  button.addEventListener('click', () => {
    invoke('open_link', { url });
  });
  return button;
}

function card(available, current) {
  const body = document.createElement('div');
  const line = document.createElement('p');
  line.className = 'update-head';
  // A tag is usually spelled `v1.0.1` and this build's number is not, so the two
  // versions on one line are put in the same form. Rust compares the tag itself.
  setText(line, 'update.head', { version: available.version.replace(/^v/, ''), current });
  body.append(line);
  if (available.notes) {
    // Written as the author left it: a release body is markdown, and the notes
    // answer "is it worth restarting for", which plain lines say well enough.
    const notes = document.createElement('p');
    notes.className = 'update-notes';
    notes.textContent = available.notes;
    body.append(notes);
  }
  const dialog = modalDialog();
  dialog.openModal('update.title', body, [openButton(available.url), closeButton(dialog)]);
}

/// Asks once a day, and only when Rust says the day has passed. Every failure
/// here is silent on purpose: a person who opened the app to make a shim has no
/// use for an error about a network that was never part of the task, and the
/// same silence is what keeps an offline machine quiet.
export async function checkForUpdate() {
  if (!(await invoke('update_due'))) {
    return;
  }
  let release = null;
  try {
    const response = await fetch(releases);
    if (response.ok) {
      const body = await response.json();
      release = { version: body.tag_name, url: body.html_url, notes: body.body ?? '' };
    }
  } catch {
    // Not an answer is still an ask, and one that went unanswered is a day's rest.
  }
  const [available, current] = await Promise.all([
    invoke('update_seen', { release }),
    window.__TAURI__.app.getVersion(),
  ]);
  if (available) {
    card(available, current);
  }
}
