import { localize, t } from './i18n.js';
import { renderSingle } from './screens/single.js';

const screens = { single: renderSingle };
const screenRoot = document.getElementById('screen');
const tabsRoot = document.getElementById('tabs');
const statusRoot = document.getElementById('status');

let current = 'single';

function render() {
  tabsRoot.replaceChildren();
  for (const [name, renderScreen] of Object.entries(screens)) {
    const tab = document.createElement('button');
    tab.type = 'button';
    tab.dataset.i18n = `tab.${name}`;
    tab.setAttribute('role', 'tab');
    tab.setAttribute('aria-selected', String(name === current));
    tab.addEventListener('click', () => {
      current = name;
      render();
    });
    tabsRoot.append(tab);
  }

  screenRoot.replaceChildren();
  screens[current](screenRoot);
  localize();
}

function formatKb(bytes) {
  return `${Math.round(bytes / 1024)} KB`;
}

async function reportEmbeddedRunners() {
  const runners = await window.__TAURI__.core.invoke('embedded_runners');
  const sizes = runners.map((runner) => `${runner.variant} ${formatKb(runner.size)}`);
  statusRoot.textContent = t('status.runners', { size: sizes.join(' / ') });
}

render();
reportEmbeddedRunners().catch((error) => {
  statusRoot.textContent = String(error);
});
