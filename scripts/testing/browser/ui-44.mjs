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

async function open(name, extra = {}) {
  const page = await (await browser.newContext({ viewport: { width: Number(width), height: 900 }, colorScheme: theme })).newPage();
  page.on('pageerror', error => { console.log('PAGE ERROR', error.message); process.exitCode = 1; });
  await page.addInitScript(`window.__AUDIT=${JSON.stringify({ theme, bigSet: true, sources: sources[name], ...extra })};`);
  await page.addInitScript(mock);
  await page.goto(url);
  await page.waitForTimeout(5500);
  await page.reload();
  await page.waitForSelector(extra.section ? 'main h1' : '.fm-row[data-id]');
  return page;
}

const shot = (page, name) => page.screenshot({ path: join(shots, `44-${theme}-${width}-${name}.png`) });
const buttons = page => page.$$eval('.activity-rail .rail-btn', list => list.map(button => button.getAttribute('aria-label').split(' · ')[0]));
const flyout = page => page.locator('.rail-flyout');
const focused = page => page.evaluate(() => document.activeElement?.textContent?.replace(/\s+/g, ' ').trim() || document.activeElement?.getAttribute('aria-label'));
const provider = page => page.locator('.rail-btn[data-provider="github"]');
const brand = page => page.locator('.shell-brand span').innerText();

const two = await open('two');
check('rail groups: Local Git, GitHub, System', JSON.stringify(await buttons(two)) === JSON.stringify(['Repositories', 'Changes', 'Branches & tags', 'Compare', 'Search', 'GitHub', 'Activity', 'Recovery', 'Settings']), JSON.stringify(await buttons(two)));
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

for (const [key, label] of [['Control+1', 'REPOSITORIES'], ['Control+2', 'CHANGES'], ['Control+3', 'BRANCHES & TAGS'], ['Control+4', 'COMPARE'], ['Control+5', 'SEARCH'], ['Control+j', 'ACTIVITY']]) {
  await two.keyboard.press(key);
  await two.waitForTimeout(200);
  check(`${key} shows ${label}`, (await brand(two)) === label, await brand(two));
}
await two.keyboard.press('Control+,');
await two.waitForTimeout(200);
check('Ctrl+, opens Settings', (await two.locator('.shell-tab.on').innerText()).includes('Settings'));
check('no horizontal overflow on the page', await two.evaluate(() => document.documentElement.scrollWidth <= innerWidth));


const rail = (page, label) => page.click(`.activity-rail .rail-btn[aria-label="${label}"]`);
const tableWidth = page => page.evaluate(() => Math.round(document.querySelector('.fm-table')?.getBoundingClientRect().width ?? -1));

await rail(two, 'Repositories');
await two.waitForSelector('.fm-row[data-id]');
await two.waitForTimeout(600);
const wide = Number(width) === 1440 ? [714, 1094] : [390, 770];
check(`Repositories table: ${wide[1]} px`, (await tableWidth(two)) === wide[1], String(await tableWidth(two)));
check('the Repositories table does not scroll sideways', await two.evaluate(() => { const box = document.querySelector('.fm-table .vbox'); return box.scrollWidth <= box.clientWidth; }));
await two.click('.side .nav:has-text("All services")');
await two.waitForSelector('.rf-setbar');
await two.waitForTimeout(600);
check(`A set table today: ${wide[0]} px with the details panel`, (await tableWidth(two)) === wide[0], String(await tableWidth(two)));
await two.click('button[aria-label="Toggle details"]');
await two.waitForTimeout(500);
check(`A set table without the details panel: ${wide[1]} px`, (await tableWidth(two)) === wide[1], String(await tableWidth(two)));
await two.click('button[aria-label="Toggle details"]');

await two.locator('.rail-btn[data-provider="github"]').click();
await two.locator('.rail-flyout [role="group"][aria-label="git.acme.example"] [role="menuitem"]').first().click();
await two.waitForSelector('.module-table .fm-row[data-id], .module-table .fm-row[role="row"]:not(.fm-head)');
await two.waitForTimeout(1500);
const pullRows = async () => Number(await two.locator('.module-table [role="grid"]').getAttribute('aria-rowcount')) - 1;
const settled = async (read, expected) => { for (let tries = 0; tries < 20; tries++) { if ((await read()) === (await expected())) return true; await two.waitForTimeout(250); } return false; };
const chipCount = async name => Number((await two.locator(`.fm-chip:has-text("${name}") b`).innerText()).trim());
check('the pull request page lists the queue', (await pullRows()) > 0 && (await settled(pullRows, async () => Math.min(25, await chipCount('All')))), `${await pullRows()} rows`);
check(`the pull request table is ${wide[1]} px wide`, (await tableWidth(two)) === wide[1], String(await tableWidth(two)));
check('the module page leaves no room for the details panel', (await two.locator('.shell-right [class]').count()) === 0 || (await two.evaluate(() => document.querySelector('#shell').classList.contains('noright'))));
await shot(two, 'prs');
await two.click('.fm-chip:has-text("Needs review")');
check('the Needs review chip narrows the rows to its count', await settled(pullRows, async () => Math.min(25, await chipCount('Needs review'))));
check('the sidebar queue count matches the chip', (await two.locator('.side .nav:has-text("Needs review") .cnt').innerText()).trim() === String(await chipCount('Needs review')));
check('queues that need author data are disabled', (await two.locator('.side .nav:has-text("Created by me")').isDisabled()) && (await two.locator('.side .nav:has-text("Assigned to me")').isDisabled()));
await two.fill('.side .gsearch input', 'zzz-nothing');
await two.waitForTimeout(400);
check('the filter box empties the table', (await pullRows()) === 0 && (await two.locator('.module-table .empty-state').count()) === 1, `${await pullRows()} rows`);
await two.fill('.side .gsearch input', '');
await two.click('.fm-chip:has-text("All")');
check('the Pull requests rail badge matches the needs-review count', (await two.locator('.rail-btn[data-provider="github"] .rail-badge').innerText()).includes(String(await chipCount('Needs review'))));

await rail(two, 'Branches & tags');
await two.waitForTimeout(500);
for (const [name, title, button] of [['Clean up branches', 'Clean up merged branches', 'Clean up'], ['Tags', 'Tags', 'Tag'], ['Stash', 'Stash', 'Stash']]) {
  await two.click(`.side .nav:has-text("${name}")`);
  await two.waitForTimeout(300);
  check(`Branches & tags: ${name} page`, (await two.locator('main h1:visible').innerText()) === title && (await two.locator(`.module-table .fm-action:has-text("${button}")`).count()) > 0);
}
await shot(two, 'branches-stash');
await two.locator('.module-table .fm-action:has-text("Stash")').first().click();
await two.waitForSelector('dialog[open]');
check('a row action opens the existing stash dialog', true);
await two.keyboard.press('Escape');

await rail(two, 'Search');
await two.waitForSelector('.code-search');
check('Search shows its mode and recent searches in the sidebar', (await two.locator('.side h6').allInnerTexts()).join('|').toLowerCase().includes('mode|recent searches'));
await two.fill('.cs-pattern input', 'retry');
await two.click('.cs-bar button[type=submit]');
await two.waitForTimeout(500);
check('a search lands in the recent list', (await two.locator('.side .nav:has-text("retry")').count()) === 1);
await shot(two, 'search');

for (const [label, title] of [['Changes', 'Changes']]) {
  await rail(two, label);
  check(`${label} shows its placeholder in the page frame`, (await two.locator('main h1:visible').innerText()) === title && (await two.locator('.module-empty').count()) === 1);
}
await shot(two, 'changes');
await two.locator('.rail-btn[data-provider="github"]').click();
await two.locator('.rail-flyout [role="menuitem"]').nth(1).click();
await two.waitForTimeout(300);
check('Actions shows its placeholder', (await two.locator('main h1:visible').innerText()) === 'Workflow runs');
await shot(two, 'actions');


const restored = await open('two', { bigSet: false, mixed: true, section: 'prs' });
await restored.waitForTimeout(3000);
check('a Pull requests page restored at launch loads its rows without a click', (await restored.locator('.module-table [role="grid"]').getAttribute('aria-rowcount')) !== '1' && (await restored.locator('.module-table .empty-state').count()) === 0, await restored.locator('.module-table [role="grid"]').getAttribute('aria-rowcount'));
check('and its sidebar is the Pull requests sidebar', (await brand(restored)) === 'PULL REQUESTS');
const total = Number(await restored.locator('.fm-chip:has-text("All") b').innerText());
await restored.locator('.rail-btn[data-provider="github"]').click();
await restored.locator('.rail-flyout [role="group"][aria-label="github.com"] [role="menuitem"]').first().click();
await restored.waitForTimeout(800);
const chipHost = restored.locator('.host-chip');
check('picking a host row scopes the page and shows a host chip', (await chipHost.count()) === 1 && (await chipHost.innerText()).includes('github.com'), await chipHost.innerText());
const hostTotal = Number(await restored.locator('.fm-chip:has-text("All") b').innerText());
check('the scope narrows the counts to that host', hostTotal > 0 && hostTotal < total, `${hostTotal} of ${total}`);
await restored.locator('.rail-btn[data-provider="github"]').click();
check('only the chosen host row is marked current', (await restored.locator('.rail-flyout .fly-item.on').count()) === 1 && (await restored.locator('.rail-flyout [aria-label="github.com"] .fly-item.on').count()) === 1);
await restored.keyboard.press('Escape');
await restored.locator('.rail-flyout').waitFor({ state: 'detached' });
await shot(restored, 'prs-host');
await restored.click('.host-chip');
await restored.waitForTimeout(500);
check('the host chip clears the scope', (await restored.locator('.host-chip').count()) === 0 && Number(await restored.locator('.fm-chip:has-text("All") b').innerText()) === total);
await restored.locator('.rail-btn[data-provider="github"]').click();
await restored.locator('.rail-flyout [role="group"][aria-label="git.acme.example"] [role="menuitem"]').nth(1).click();
await restored.waitForTimeout(400);
check('Actions accepts the host too', (await restored.locator('main h1:visible').innerText()) === 'Workflow runs' && (await restored.locator('.host-chip').innerText()).includes('git.acme.example'));
await restored.keyboard.press('Control+j');
await restored.waitForTimeout(200);
await restored.click('.shell-tab:has-text("Repositories") button[role="tab"]');
await restored.waitForTimeout(300);
check('opening a set tab leaves the Activity sidebar in place', (await brand(restored)) === 'ACTIVITY' && (await restored.locator('main .fm-table').count()) > 0);
await restored.click('.rail-btn[aria-label="Recovery"]');
await restored.click('.shell-tab:has-text("Pull requests") button[role="tab"]');
check('and the Recovery sidebar too', (await brand(restored)) === 'RECOVERY');

const repos = await open('two', { bigSet: false });
const chipN = async name => Number((await repos.locator(`.fm-chip:has-text("${name}") b`).first().innerText()).trim());
const setCount = async name => Number((await repos.locator(`.side .nav:has-text("${name}") .cnt`).first().innerText()).trim());
await repos.waitForFunction(() => document.querySelector('.fm-chip b')?.textContent === '806');
check('Repositories is home and lists cloned and remote-only rows', (await chipN('All')) > (await chipN('Cloned')) && (await repos.locator('.fm-row[data-id]').count()) > 0);
check('the known count covers every source', (await chipN('All')) === 800 + 6, String(await chipN('All')));
check('Cloned counts the folders on disk', (await chipN('Cloned')) > 0 && (await chipN('Cloned')) < 30, String(await chipN('Cloned')));
check('Favorites counts the starred repositories', (await chipN('Favorites')) === 2, String(await chipN('Favorites')));
check('the sidebar lists the favorites', (await repos.locator('.side .nav.fav').count()) === 2);
check('the sidebar tree lists both hosts with their organizations', (await repos.locator('.side .rf-hostbtn').allInnerTexts()).length === 2 && (await repos.locator('.side .nav.p-sub').count()) === 5);
await shot(repos, 'repos');
await repos.fill('.rf-search input', 'gateway');
await repos.waitForTimeout(300);
check('the name filter narrows the table and the chips', (await chipN('All')) < 200 && (await chipN('All')) > 0, String(await chipN('All')));
await repos.fill('.rf-search input', '');
await repos.click('.side .rf-hostbtn:has-text("github.com")');
await repos.waitForTimeout(300);
check('a host in the tree filters the table and shows a removable chip', (await chipN('All')) === 6 && (await repos.locator('.fm-chip.on:has-text("github.com")').count()) === 1, String(await chipN('All')));
await repos.click('.fm-chip.on:has-text("github.com")');
await repos.click('.fm-chip:has-text("Favorites")');
await repos.waitForTimeout(300);
check('the Favorites chip shows only starred rows', (await repos.locator('.fm-row[data-id]').count()) === 2);
await repos.click('.fm-chip:has-text("All")');

await repos.fill('.rf-search input', 'cli');
await repos.waitForTimeout(300);
const remoteRow = repos.locator('.fm-row:has(.fm-remote)').first();
check('a remote-only row says remote and offers Clone', (await remoteRow.locator('.fm-action').innerText()).trim() === 'Clone' && (await remoteRow.locator('.fm-quiet').innerText()).includes('Not cloned'));
const before = await setCount('Release train');
await remoteRow.locator('.fm-action').click();
await repos.waitForTimeout(500);
check('Clone on a remote row adds it to the active set', (await setCount('Release train')) === before + 1, `${before} -> ${await setCount('Release train')}`);
await repos.fill('.rf-search input', 'sdk');
await repos.waitForTimeout(300);
await repos.locator('.fm-row:has(.fm-remote) .fm-more button').first().click();
await repos.click('.row-menu button:has-text("Add to set")');
await repos.click('.popover .menu-item:has-text("Empty set")');
await repos.waitForTimeout(300);
check('Add to set from the row menu fills the chosen set', (await setCount('Empty set')) === 1, String(await setCount('Empty set')));
await repos.fill('.rf-search input', '');

await repos.click('.rf-setchip');
await repos.click('.popover .menu-item:has-text("Mobile hotfix")');
await repos.waitForSelector('.rf-setbar');
check('Any set filters to a set and shows the set bar', (await repos.locator('.shell-tab.on').innerText()).includes('Mobile hotfix') && (await repos.locator('.rf-setbar .btn').allInnerTexts()).join('|').replace(/\s+/g, ' ').includes('Compare set'), (await repos.locator('.rf-setbar .btn').allInnerTexts()).join('|'));
await shot(repos, 'set-bar');
await repos.click('.rf-setbar button:has-text("Delete set"), .rf-setbar button[aria-label="Delete set"]');
await repos.waitForSelector('dialog[open]');
check('deleting a set says it is local only', (await repos.locator('dialog[open] p').first().innerText()).includes('Local only. Nothing on github.com changes.'), await repos.locator('dialog[open] p').first().innerText());
await repos.click('dialog[open] button:has-text("Cancel")');
await repos.click('.rf-setbar button[aria-label="Leave this set"]');
await repos.waitForTimeout(300);
check('leaving the set returns to every repository', (await repos.locator('.shell-tab.on').innerText()).includes('Repositories') && (await chipN('All')) === 806);

const github = await open('github');
check('one GitHub host shows one section', (await (async () => { await provider(github).click(); return github.locator('.rail-flyout [role="group"]').count(); })()) === 1);
check('Jira stays off the rail without a Jira source', !(await buttons(github)).some(label => label.startsWith('Jira')));

const manual = await open('manual');
check('no provider button without a GitHub or Jira source', (await buttons(manual)).join() === 'Repositories,Changes,Branches & tags,Compare,Search,Activity,Recovery,Settings', (await buttons(manual)).join());

const jira = await open('jira');
check('a Jira source adds the Jira button', (await buttons(jira)).some(label => label.startsWith('Jira')));
await jira.locator('.rail-btn[data-provider="jira"]').click();
await jira.waitForTimeout(300);
await shot(jira, 'flyout-jira');
await jira.keyboard.press('ArrowDown');
await jira.keyboard.press('Enter');
await jira.waitForTimeout(300);
check('Jira opens its placeholder page', (await brand(jira)) === 'JIRA' && (await jira.locator('main h1:visible').innerText()) === 'Jira issues');

await browser.close();
