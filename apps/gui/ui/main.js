import { aboutDialog } from './about.js';
import { dropHandler } from './drop.js';
import { formatSize } from './form.js';
import { localize, loadLanguage, setText } from './i18n.js';
import { renderBatch } from './screens/batch.js';
import { renderSingle } from './screens/single.js';
import { settingsSheet } from './settings.js';
import { loadTheme } from './theme.js';
import { titlebar } from './titlebar.js';
import { checkForUpdate } from './update.js';

const screens = { single: renderSingle, batch: renderBatch };
const screenRoot = document.getElementById('screen');
const tabsRoot = document.getElementById('tabs');
const statusRoot = document.getElementById('status');

const about = aboutDialog();
const settings = settingsSheet();

let current = 'single';
const tabs = new Map();
const panels = new Map();
// The path fields each screen hands out, which is what a drop is aimed at.
const fields = {};

// Both screens are built once and then shown or hidden, so a half-written form
// and a scan someone is still ticking survive a look at the other mode;
// rebuilding the widgets would answer a tab click by throwing that away.
function build() {
  for (const [name, renderScreen] of Object.entries(screens)) {
    const tab = document.createElement('button');
    tab.type = 'button';
    tab.id = `tab-${name}`;
    tab.dataset.i18n = `tab.${name}`;
    tab.setAttribute('role', 'tab');
    tab.setAttribute('aria-controls', `panel-${name}`);
    tab.addEventListener('click', () => show(name));
    tabsRoot.append(tab);
    tabs.set(name, tab);

    const panel = document.createElement('div');
    panel.id = `panel-${name}`;
    panel.hidden = name !== current;
    panel.setAttribute('role', 'tabpanel');
    panel.setAttribute('aria-labelledby', `tab-${name}`);
    fields[name] = renderScreen(panel);
    screenRoot.append(panel);
    panels.set(name, panel);
  }
}

function show(name) {
  current = name;
  for (const [each, tab] of tabs) {
    tab.setAttribute('aria-selected', String(each === name));
  }
  for (const [each, panel] of panels) {
    panel.hidden = each !== name;
  }
}

// Both are read before anything is built, so the window opens in the language and
// the theme it is going to keep rather than showing the change on the way.
await Promise.all([loadLanguage(), loadTheme()]);

build();
// Built before the translation pass, so the brand it carries is filled in too.
titlebar({ onSettings: () => settings.show(), onAbout: () => about.show() });
show(current);
localize();

// Tauri answers a drag over the window with an event of its own instead of the
// page's `drop`, so this listener is the only way a path can reach the form.
const handleDrop = dropHandler({ screens: fields, show });
await window.__TAURI__.webview
  .getCurrentWebview()
  .onDragDropEvent((event) => handleDrop(event.payload));

async function reportEmbeddedRunners() {
  const runners = await window.__TAURI__.core.invoke('embedded_runners');
  const sizes = runners.map((runner) => `${runner.variant} ${formatSize(runner.size)}`);
  setText(statusRoot, 'status.runners', { size: sizes.join(' / ') });
}

reportEmbeddedRunners().catch((error) => {
  statusRoot.textContent = String(error);
});

// Left to run after the window is up rather than waited for: the screen someone
// opened the app for is not worth hiding behind a card about a version, and a
// network that answers slowly should not hold the form. A rejection here would be
// a message about a check nobody asked to see, so it goes unheard.
setTimeout(() => {
  checkForUpdate().catch(() => {});
}, 2000);
