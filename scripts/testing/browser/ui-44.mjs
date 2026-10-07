// Rail, provider flyout and module pages in Helium with mocked Tauri commands (800-repo set, two GitHub hosts).
// Usage: PLAYWRIGHT_DIR=<dir with playwright-core> node ui-44.mjs <url> <shots-dir> [light|dark] [width]
import { mkdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const [url, shots, theme = 'light', width = '1440'] = process.argv.slice(2);
const { chromium } = await import(`${process.env.PLAYWRIGHT_DIR}/index.mjs`);
const mock = readFileSync(join(dirname(fileURLToPath(import.meta.url)), 'shell-mock.js'), 'utf8');
const sources = {
  github: [{ id: 's2', name: 'github.com', kind: 'github', host: 'github.com', orgs: ['acme-oss'], urls: [] }],
  two: [{ id: 's1', name: 'Acme GHE', kind: 'ghe', host: 'git.acme.example', orgs: ['platform'], urls: [] }, { id: 's2', name: 'github.com', kind: 'github', host: 'github.com', orgs: ['acme-oss'], urls: [] }],
  jira: [{ id: 's2', name: 'github.com', kind: 'github', host: 'github.com', orgs: [], urls: [] }, { id: 'j1', name: 'Jira', kind: 'jira', host: 'jira.acme.example', orgs: [], urls: [] }],
  manual: [{ id: 'm1', name: 'Local', kind: 'manual', host: '', orgs: [], urls: [] }],
};

const browser = await chromium.launch({ executablePath: '/opt/helium-browser-bin/helium', headless: true, args: ['--no-sandbox'] });
mkdirSync(shots, { recursive: true });
const check = (label, ok, detail = '') => { console.log(`${ok ? 'PASS' : 'FAIL'} ${label} ${detail}`); if (!ok) process.exitCode = 1; };

async function open(name) {
  const page = await (await browser.newContext({ viewport: { width: Number(width), height: 900 }, colorScheme: theme })).newPage();
  page.on('pageerror', error => { console.log('PAGE ERROR', error.message); process.exitCode = 1; });
  await page.addInitScript(`window.__AUDIT=${JSON.stringify({ theme, bigSet: true, sources: sources[name] })};`);
  await page.addInitScript(mock);
  await page.goto(url);
  await page.waitForTimeout(5500);
  await page.reload();
  await page.waitForSelector('.fm-row[data-id]');
  return page;
}

const shot = (page, name) => page.screenshot({ path: join(shots, `44-${theme}-${width}-${name}.png`) });
const buttons = page => page.$$eval('.activity-rail .rail-btn', list => list.map(button => button.getAttribute('aria-label')));
const flyout = page => page.locator('.rail-flyout');
const focused = page => page.evaluate(() => document.activeElement?.textContent?.replace(/\s+/g, ' ').trim() || document.activeElement?.getAttribute('aria-label'));
const provider = page => page.locator('.rail-btn[data-provider="github"]');
const brand = page => page.locator('.shell-brand span').innerText();

const two = await open('two');
check('rail groups: Local Git, GitHub, System', JSON.stringify(await buttons(two)) === JSON.stringify(['Sets', 'Changes', 'Branches & tags', 'Compare', 'Search', 'GitHub, github.com, git.acme.example', 'Activity', 'Recovery', 'Settings']), JSON.stringify(await buttons(two)));
check('the provider button announces a menu', (await provider(two).getAttribute('aria-haspopup')) === 'menu' && (await provider(two).getAttribute('aria-expanded')) === 'false');

await two.waitForSelector('.rail-btn[data-provider="github"] .rail-badge');
await provider(two).hover();
await two.waitForTimeout(60);
check('hover alone does not open the flyout before the delay', (await flyout(two).count()) === 0);
await two.waitForTimeout(250);
check('hover opens the flyout after 150 ms', (await flyout(two).count()) === 1);
const hosts = await two.$$eval('.rail-flyout [role="group"]', list => list.map(group => ({ host: group.getAttribute('aria-label'), items: [...group.querySelectorAll('[role="menuitem"]')].map(item => item.textContent.replace(/\s+/g, ' ').trim()) })));
check('one section per host, github.com first, each with three items', hosts.length === 2 && hosts[0].host === 'github.com' && hosts[1].host === 'git.acme.example' && hosts.every(entry => entry.items.length === 3), JSON.stringify(hosts));
check('counts show per host', hosts[1].items[0].startsWith('Pull requests') && /\d/.test(hosts[1].items[0]) && !/\d/.test(hosts[0].items[0]), JSON.stringify(hosts.map(entry => entry.items[0])));
await shot(two, 'flyout-two-hosts');

const box = await flyout(two).boundingBox();
const start = await provider(two).boundingBox();
const [sx, sy] = [start.x + start.width - 4, start.y + start.height / 2];
await two.mouse.move(sx, sy);
for (let step = 1; step <= 10; step++) await two.mouse.move(sx + ((box.x + 40 - sx) * step) / 10, sy + (((box.y + box.height - 30) - sy) * step) / 10);
await two.waitForTimeout(400);
check('a diagonal path through the gap keeps the flyout open', (await flyout(two).count()) === 1);
await two.mouse.move(700, 500, { steps: 6 });
await two.waitForTimeout(500);
check('leaving the flyout closes it', (await flyout(two).count()) === 0);

await provider(two).hover();
await two.mouse.move(700, 400);
await two.waitForTimeout(400);
check('a pass over the button shorter than the delay never opens it', (await flyout(two).count()) === 0);

await provider(two).focus();
await two.keyboard.press('ArrowRight');
check('ArrowRight opens the flyout and focuses the first item', (await flyout(two).count()) === 1 && (await focused(two)) === 'Pull requests', await focused(two));
await two.keyboard.press('ArrowDown');
check('ArrowDown moves to the next item', (await focused(two)) === 'Actions', await focused(two));
await two.keyboard.press('End');
check('End moves to the last item, across hosts', (await focused(two)) === 'Releases' && (await two.evaluate(() => document.activeElement.closest('[role="group"]').getAttribute('aria-label'))) === 'git.acme.example');
await two.keyboard.press('ArrowDown');
check('ArrowDown wraps to the first item', (await two.evaluate(() => document.activeElement.closest('[role="group"]').getAttribute('aria-label'))) === 'github.com' && (await focused(two)) === 'Pull requests');
await two.keyboard.press('Home');
await two.keyboard.press('r');
check('typeahead jumps to Releases', (await focused(two)) === 'Releases', await focused(two));
await two.waitForTimeout(700);
await two.keyboard.press('a');
check('typeahead jumps to Actions', (await focused(two)) === 'Actions', await focused(two));
await two.keyboard.press('Escape');
check('Escape closes the flyout and returns focus to the rail button', (await flyout(two).count()) === 0 && (await two.evaluate(() => document.activeElement?.getAttribute('data-provider'))) === 'github');
await two.keyboard.press('Enter');
check('Enter opens it again with focus inside', (await flyout(two).count()) === 1 && (await focused(two)) === 'Pull requests');
await two.keyboard.press('ArrowLeft');
check('ArrowLeft closes and returns focus', (await flyout(two).count()) === 0 && (await two.evaluate(() => document.activeElement?.getAttribute('data-provider'))) === 'github');
await two.keyboard.press('Space');
check('Space opens it', (await flyout(two).count()) === 1);
await two.keyboard.press('Enter');
await two.waitForSelector('.shell-tab.on');
check('Enter on an item opens the module', (await brand(two)) === 'PULL REQUESTS' && (await flyout(two).count()) === 0, await brand(two));
check('focus returns to the rail button after a pick', (await two.evaluate(() => document.activeElement?.getAttribute('data-provider'))) === 'github');

await provider(two).click();
check('click pins the flyout open', (await flyout(two).count()) === 1);
await two.mouse.move(700, 400, { steps: 4 });
await two.waitForTimeout(500);
check('a pinned flyout stays open when the pointer leaves', (await flyout(two).count()) === 1);
await two.mouse.click(700, 400);
await two.waitForTimeout(200);
check('a click outside closes it', (await flyout(two).count()) === 0);

for (const [key, label] of [['Control+1', 'SETS'], ['Control+2', 'CHANGES'], ['Control+3', 'BRANCHES & TAGS'], ['Control+4', 'COMPARE'], ['Control+5', 'SEARCH'], ['Control+j', 'ACTIVITY']]) {
  await two.keyboard.press(key);
  await two.waitForTimeout(200);
  check(`${key} shows ${label}`, (await brand(two)) === label, await brand(two));
}
await two.keyboard.press('Control+,');
await two.waitForTimeout(200);
check('Ctrl+, opens Settings', (await two.locator('.shell-tab.on').innerText()).includes('Settings'));
check('no horizontal overflow on the page', await two.evaluate(() => document.documentElement.scrollWidth <= innerWidth));

const github = await open('github');
check('one GitHub host shows one section', (await (async () => { await provider(github).click(); return github.locator('.rail-flyout [role="group"]').count(); })()) === 1);
check('Jira stays off the rail without a Jira source', !(await buttons(github)).some(label => label.startsWith('Jira')));

const manual = await open('manual');
check('no provider button without a GitHub or Jira source', (await buttons(manual)).join() === 'Sets,Changes,Branches & tags,Compare,Search,Activity,Recovery,Settings', (await buttons(manual)).join());

const jira = await open('jira');
check('a Jira source adds the Jira button', (await buttons(jira)).some(label => label.startsWith('Jira')));
await jira.locator('.rail-btn[data-provider="jira"]').click();
await jira.waitForTimeout(300);
await shot(jira, 'flyout-jira');
await jira.keyboard.press('ArrowDown');
await jira.keyboard.press('Enter');
await jira.waitForTimeout(300);
check('Jira opens its placeholder page', (await brand(jira)) === 'JIRA' && (await jira.locator('main h1').innerText()) === 'Jira issues');

await browser.close();
