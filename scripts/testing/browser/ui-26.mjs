// Stash UI and switch with stash in Helium with mocked Tauri commands backed by real Git fixtures.
// Usage: PLAYWRIGHT_DIR=<dir with playwright-core> node ui-26.mjs <url> <shots-dir> [light|dark] [width]
import { mkdtempSync, mkdirSync, existsSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { localStatus } from './backend.mjs';
import { git } from './fixture.mjs';
import * as stash from './stash-backend.mjs';
import { makeStashSet } from './stash-fixture.mjs';
import { clearNarrow } from './shell-nav.mjs';

const [url, shots, theme = 'light', width = '1440'] = process.argv.slice(2);
const { chromium } = await import(`${process.env.PLAYWRIGHT_DIR}/index.mjs`);
const root = mkdtempSync(join(tmpdir(), 'skein-ui26-'));
const repos = makeStashSet(root);
const caps = Object.fromEntries(['readCompare', 'edit', 'copy', 'recovery', 'trash'].map(name => [name, { supported: true, reason: null }]));
const items = Object.keys(repos).map(name => ({ id: name, repoId: 'fixture:' + name, url: `https://example.test/fixture/${name}.git`, org: 'fixture', name, ref: { type: 'branch', name: 'release' }, on: false }));
const workspace = { sets: [{ id: 'fix', name: 'Fixture set', items }], activeSet: 'fix', root, theme, pageSize: 'all' };
const source = { id: 'fixture', name: 'Fixture', kind: 'manual', host: '', orgs: [], urls: [] };

const browser = await chromium.launch({ executablePath: '/opt/helium-browser-bin/helium', headless: true, args: ['--no-sandbox'] });
const page = await (await browser.newContext({ viewport: { width: Number(width), height: 900 } })).newPage();
page.on('pageerror', error => console.log('PAGE ERROR', error.message));
page.on('console', message => { if (['error', 'warning'].includes(message.type())) console.log('CONSOLE', message.type(), message.text()); });
const unknown = new Set();

await page.exposeFunction('__backend', async (cmd, a) => {
  switch (cmd) {
    case 'load_settings': return { sources: [source], workspace };
    case 'platform_info': return { platform: 'linux', separator: '/', capabilities: caps, credentials: { backend: 'unsupported', persistent: false, supported: false, reason: null } };
    case 'probe_root': return { root: a.root, valid: true, reason: null, identity: 'fixture', casePolicy: 'sensitive', capabilities: caps };
    case 'path_identities': return a.paths.map(path => ({ path, identity: path, exists: true, reason: null }));
    case 'paths_exist': return a.paths.map(() => true);
    case 'local_status': return localStatus(a.paths);
    case 'stash_list': return stash.list(a.path);
    case 'stash_push': return stash.push(a.path, a.message, a.includeUntracked);
    case 'stash_apply': return stash.apply(a.path, a.oid);
    case 'stash_pop': return stash.pop(a.path, a.oid);
    case 'stash_drop': return stash.drop(a.path, a.oid) ?? null;
    case 'stash_show': return stash.show(a.path, a.oid);
    case 'switch_with_stash': return stash.switchWithStash(a.path, a.branch);
    case 'pull_for_branch': return null;
    case 'activity_snapshot': case 'get_refs_many': case 'list_tags': return [];
    case 'list_repos': return { repos: [], fetchedAt: 0, errors: [] };
    case 'open_in_vscode': return null;
    case 'save_settings': case 'list_cached_repos': case 'plugin:window|set_theme': return null;
    case 'source_revision': return 0;
    case 'credential_status': return { sourceId: a.sourceId, backend: 'unsupported', state: 'unavailable', revision: 0, reason: null };
    case 'repository_history': return { kind: 'tracking', branch: git(a.path, 'branch', '--show-current').trim(), upstream: null, uncommitted: 0, local: [], localTotal: 0, origin: [], originTotal: 0, base: null, below: [] };
    case 'repository_tree': return { branches: [], tags: [], remotes: [], stashes: [], submodules: [] };
    default: unknown.add(cmd); return null;
  }
});
await page.addInitScript(() => {
  let next = 1;
  const listeners = {};
  window.__TAURI_INTERNALS__ = {
    transformCallback(fn) { const id = next++; window['_' + id] = fn; return id; },
    async invoke(cmd, args) {
      if (cmd === 'plugin:event|listen') { (listeners[args.event] ??= []).push(args.handler); return listeners[args.event].length; }
      if (cmd === 'plugin:event|unlisten') return null;
      try { return await window.__backend(cmd, args ?? {}); } catch (error) { throw String(error.message ?? error).replace(/^Error: /, ''); }
    },
    convertFileSrc: path => path,
    metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main', windowLabel: 'main' } },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener(event, id) { const list = listeners[event]; if (list) list[id - 1] = null; } };
  window.__emit = (event, payload) => (listeners[event] ?? []).forEach((handler, index) => handler && window['_' + handler]({ event, id: index + 1, payload }));
});

mkdirSync(shots, { recursive: true });
const shot = async name => { await page.waitForTimeout(450); return page.screenshot({ path: join(shots, `26-${theme}-${width}-${name}.png`) }); };
const check = (label, ok, detail = '') => { console.log(`${ok ? 'PASS' : 'FAIL'} ${label} ${detail}`); if (!ok) process.exitCode = 1; };
const branch = name => git(repos[name], 'branch', '--show-current').trim();
const stashes = name => stash.list(repos[name]);
const row = name => page.locator(`.fm-row[data-id="${name}"]`);

await page.goto(url);
await page.waitForTimeout(5500);
await page.waitForSelector('.fm-row[data-id="alpha"]');
await clearNarrow(page, width);
await page.waitForTimeout(800);

// Row menu: disabled reasons on the clean repository, enabled on a dirty one.
await row('gamma').hover();
await page.click('button[aria-label="More actions for gamma"]');
const gammaStash = page.locator('[role=menuitem]:has-text("Stash changes")');
check('stash disabled on clean repo with reason', (await gammaStash.isDisabled()) && (await gammaStash.getAttribute('title')) === 'No uncommitted changes', await gammaStash.getAttribute('title'));
await shot('row-menu-clean');
await page.keyboard.press('Escape');
await row('alpha').hover();
await page.click('button[aria-label="More actions for alpha"]');
check('stash and switch enabled on dirty off-branch repo', !(await page.locator('[role=menuitem]:has-text("Stash changes")').isDisabled()) && !(await page.locator('[role=menuitem]:has-text("Switch with stash")').isDisabled()));
await shot('row-menu-dirty');
await page.keyboard.press('Escape');

// Bulk: select all, More menu.
await page.check('.fm-head input[type=checkbox]');
await page.waitForSelector('.fm-bar.on');
await page.click('.fm-bar .more');
const menuText = await page.locator('.bulk-more').innerText();
check('More menu has both entries', menuText.includes('Switch with stash') && menuText.includes('Stash changes'), menuText.replace(/\n/g, ' | '));
await shot('bulk-more');
await page.click('[role=menuitem]:has-text("Stash changes")');
await page.waitForSelector('.stash-dialog[open]');
check('bulk stash dialog lists the two dirty repositories', (await page.locator('.stash-targets li').count()) === 2);
await shot('bulk-stash');
await page.click('.stash-dialog footer .btn:not(.dark)');
await page.waitForTimeout(500);
check('focus returns to More after cancelling the stash dialog', await page.evaluate(() => document.activeElement?.classList.contains('more')));
await page.click('.fm-bar .more');
await page.click('[role=menuitem]:has-text("Switch with stash")');
await page.waitForSelector('.stash-dialog[open]');
const review = await page.locator('.stash-dialog').innerText();
check('review lists three repositories and the stash note', ['alpha', 'beta', 'gamma'].every(name => review.includes(name)) && review.includes('Nothing is dropped'), review.slice(0, 80));
await shot('switch-review');
await page.dblclick('.stash-dialog footer .btn.dark');
await page.waitForSelector('.stash-restore');
check('double click ran one loop', stash.calls.filter(call => call.command === 'switch_with_stash').length === 3, String(stash.calls.filter(call => call.command === 'switch_with_stash').length));
await page.waitForTimeout(500);
check('all three switched', ['alpha', 'beta', 'gamma'].every(name => branch(name) === 'release'), ['alpha', 'beta', 'gamma'].map(branch).join());
check('two stashes created, none for clean gamma', stashes('alpha').some(entry => entry.message.includes('before switching to release')) && stashes('beta').some(entry => entry.message.includes('before switching to release')) && !stashes('gamma').some(entry => entry.message.includes('before switching')));
const results = await page.locator('.stash-dialog [aria-label=Results]').innerText();
check('result list names each outcome', results.includes('Switched, changes stashed') && results.includes('Switched'), results.replace(/\n/g, ' | '));
check('restore step lists only stashed repositories', (await page.locator('.stash-restore li').count()) === 2);
await shot('switch-result');

await page.click('.stash-restore-head .btn');
await page.waitForSelector('.stash-restore .banner.err');
await page.waitForTimeout(500);
check('alpha restored with untracked file back', readFileSync(join(repos.alpha, 'src/app.ts'), 'utf8').includes('two = 22') && existsSync(join(repos.alpha, 'notes-untracked.txt')));
check('alpha stash kept after apply', stashes('alpha').some(entry => entry.message.includes('before switching')));
const conflictText = await page.locator('.stash-restore .banner.err').innerText();
check('beta conflict lists the file and keeps the stash', conflictText.includes('conflict.txt') && conflictText.includes('stash is kept') && stashes('beta').some(entry => entry.message.includes('before switching')), conflictText.replace(/\n/g, ' '));
check('Open repository offered', (await page.locator('.stash-restore button:has-text("Open repository")').count()) === 1);
await shot('restore-conflict');

// Drop only after a clean apply, with a confirmation that names the stash.
await page.click('.stash-restore button:has-text("Drop stash")');
await page.waitForSelector('.confirm-dialog[open]');
const confirmText = await page.locator('.confirm-dialog').innerText();
check('drop confirmation names stash and repository', confirmText.includes('before switching to release') && confirmText.includes('alpha'), confirmText.slice(0, 100));
await shot('drop-confirm');
await page.click('.confirm-dialog button:has-text("Cancel")');
await page.waitForTimeout(300);
check('cancel keeps the stash', stashes('alpha').some(entry => entry.message.includes('before switching')));
await page.click('.stash-restore button:has-text("Drop stash")');
await page.waitForSelector('.confirm-dialog[open]');
await page.click('.confirm-dialog .btn.danger');
await page.waitForTimeout(600);
check('confirmed drop removes only that stash', !stashes('alpha').some(entry => entry.message.includes('before switching')) && stashes('beta').length === 1);
await page.click('.stash-dialog footer .btn.dark');
await page.waitForTimeout(700);
check('focus returns to the More button after the switch dialog', await page.evaluate(() => document.activeElement?.classList.contains('more')), await page.evaluate(() => document.activeElement?.outerHTML.slice(0, 60)));

// Drawer on gamma: list, preview with truncation notice, apply, pop, new stash.
await row('gamma').hover();
await page.click('button[aria-label="More actions for gamma"]');
await page.click('[role=menuitem]:has-text("History")');
await page.waitForSelector('.stash-section .stash-entry');
check('drawer lists both gamma stashes', (await page.locator('.stash-entry').count()) === 2);
await page.click('.stash-entry:has-text("Large generated file") .stash-title');
await page.waitForSelector('.stash-patch');
const notice = await page.locator('.stash-diff .banner').innerText();
check('truncation notice shown', notice.includes('Stash too large to preview'), notice.replace(/\n/g, ' '));
await shot('drawer-preview');
await page.click('.stash-entry:has-text("Tweak app value") button:has-text("Apply")');
await page.waitForSelector('.stash-entry:has-text("Tweak app value") .banner.ok');
check('apply keeps the stash and offers drop', stashes('gamma').length === 2 && (await page.locator('.stash-entry:has-text("Tweak app value") button:has-text("Drop stash")').count()) === 1);
await shot('drawer-applied');
await page.click('.stash-section .stash-head .btn');
await page.waitForSelector('.stash-dialog[open]');
await page.fill('.stash-dialog input', 'Second tweak');
await shot('push-dialog');
await page.click('.stash-dialog footer .btn.dark');
await page.waitForSelector('.stash-entry:has-text("Second tweak")');
await page.waitForTimeout(900);
check('focus stays inside the drawer after stashing', await page.evaluate(() => !!document.activeElement?.closest('.history-drawer')), await page.evaluate(() => document.activeElement?.outerHTML.slice(0, 60)));
check('new stash listed and tree cleaned', stashes('gamma').length === 3 && git(repos.gamma, 'status', '--porcelain').trim() === '');
await page.click('.stash-entry:has-text("Second tweak") button:has-text("Pop")');
await page.waitForTimeout(700);
check('pop removes the stash after a clean apply', stashes('gamma').length === 2 && !stashes('gamma').some(entry => entry.message === 'Second tweak'));
await shot('drawer-final');
console.log(unknown.size ? `UNMOCKED ${[...unknown].join(',')}` : 'no unmocked commands');
await browser.close();
