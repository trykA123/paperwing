// Tags UI in Helium with mocked Tauri commands backed by real Git fixtures and bare remotes.
// Usage: PLAYWRIGHT_DIR=<dir with playwright-core> node ui-33.mjs <url> <shots-dir> [light|dark] [width]
import { mkdtempSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { localStatus } from './backend.mjs';
import { git } from './fixture.mjs';
import * as tags from './tag-backend.mjs';
import { addCommit, makeTagSet } from './tag-fixture.mjs';

const [url, shots, theme = 'light', width = '1440'] = process.argv.slice(2);
const { chromium } = await import(`${process.env.PLAYWRIGHT_DIR}/index.mjs`);
const root = mkdtempSync(join(tmpdir(), 'skein-ui33-'));
const { paths: repos, remotes } = makeTagSet(root);
const caps = Object.fromEntries(['readCompare', 'edit', 'copy', 'recovery', 'trash'].map(name => [name, { supported: true, reason: null }]));
const items = Object.keys(repos).map(name => ({ id: name, repoId: 'fixture:' + name, url: `https://example.test/fixture/${name}.git`, org: 'fixture', name, ref: { type: 'branch', name: 'main' }, on: false }));
const narrow = Number(width) < 700;
const shell = { version: 1, sidebarWidth: 250, sidebarVisible: !narrow, rightVisible: !narrow, section: 'sets' };
const workspace = { sets: [{ id: 'fix', name: 'Fixture set', items }], activeSet: 'fix', root, theme, pageSize: 'all', shell };
const source = { id: 'fixture', name: 'Fixture', kind: 'manual', host: '', orgs: [], urls: [] };
const refCalls = [];

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
    case 'list_tags': return tags.list(a.path);
    case 'create_tag': return tags.create(a.path, a.request);
    case 'push_tag': return tags.push(a.path, a.remote, a.name, a.lease);
    case 'delete_tag': return tags.remove(a.path, a.name);
    case 'delete_remote_tag': return tags.removeRemote(a.path, a.remote, a.name, a.expected);
    case 'repository_history': return tags.history(a.path);
    case 'get_refs_many': refCalls.push(a.urls); return a.urls.map(refUrl => ({ url: refUrl, branches: ['main'], tags: [], branchShas: [], tagShas: [], error: null }));
    case 'repository_tree': refCalls.push(['tree', a.path]); return { branches: [], tags: tags.list(a.path).map(tag => ({ name: tag.name, sha: tag.commit })), remotes: [], stashes: [], submodules: [] };
    case 'activity_snapshot': case 'stash_list': return [];
    case 'list_repos': return { repos: [], fetchedAt: 0, errors: [] };
    case 'open_in_vscode': return null;
    case 'save_settings': case 'list_cached_repos': case 'plugin:window|set_theme': return null;
    case 'source_revision': return 0;
    case 'credential_status': return { sourceId: a.sourceId, backend: 'unsupported', state: 'unavailable', revision: 0, reason: null };
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
const shot = async name => { await page.waitForTimeout(450); return page.screenshot({ path: join(shots, `33-${theme}-${width}-${name}.png`) }); };
const check = (label, ok, detail = '') => { console.log(`${ok ? 'PASS' : 'FAIL'} ${label} ${detail}`); if (!ok) process.exitCode = 1; };
const names = repo => tags.list(repos[repo]).map(tag => tag.name);
const remoteNames = repo => git(remotes[repo], 'tag', '--list').split('\n').filter(Boolean);
const row = name => page.locator(`.fm-row[data-id="${name}"]`);
const countOf = command => tags.calls.filter(call => call.command === command).length;
const dialogText = () => page.locator('.tag-dialog').innerText();

await page.goto(url);
await page.waitForTimeout(5500);
await page.waitForSelector('.fm-row[data-id="alpha"]');
await page.waitForTimeout(800);

// Row menu entry on one repository.
await row('alpha').hover();
await page.click('button[aria-label="More actions for alpha"]');
check('row menu offers New tag and Delete tag', (await page.locator('[role=menuitem]:has-text("New tag")').count()) === 1 && (await page.locator('[role=menuitem]:has-text("Delete tag")').count()) === 1);
await shot('row-menu');
await page.click('[role=menuitem]:has-text("New tag")');
await page.waitForSelector('.tag-dialog[open]');
check('single-repository dialog names the repository', (await dialogText()).includes('alpha'));
await shot('single-dialog');
await page.keyboard.press('Escape');
await page.waitForTimeout(500);
check('focus returns to the row action button', await page.evaluate(() => !!document.activeElement?.closest('.fm-row')), await page.evaluate(() => document.activeElement?.outerHTML.slice(0, 60)));

// Bulk: tag the set, push, one push fails.
await page.check('.fm-head input[type=checkbox]');
await page.waitForSelector('.fm-bar.on');
await page.click('.fm-bar .more');
const menuText = await page.locator('.bulk-more').innerText();
check('More menu lists Tag and Delete tag', menuText.includes('Tag…') && menuText.includes('Delete tag…'), menuText.replace(/\n/g, ' | '));
await shot('bulk-more');
await page.click('[role=menuitem]:has-text("Tag…")');
await page.waitForSelector('.tag-dialog[open]');
await page.fill('.tag-dialog input.mono', 'v2.4.0');
await page.fill('.tag-dialog textarea', 'Release 2.4.0');
await page.check('.tag-dialog .check:has-text("Push after creating") input');
await page.waitForTimeout(500);
const review = await dialogText();
check('preview lists three repositories and no clash', ['alpha', 'beta', 'gamma'].every(name => review.includes(name)) && !review.includes('Refused'), review.replace(/\n/g, ' | ').slice(0, 120));
check('remote is shown per repository', (await page.locator('.tag-remote').count()) === 3 && (await page.locator('.tag-remote').first().inputValue()) === 'origin');
await shot('bulk-review');
await page.dblclick('.tag-dialog footer .btn.dark');
await page.waitForSelector('.tag-lead:has-text("Tagged")');
await page.waitForTimeout(500);
check('double click ran one loop', countOf('create_tag') === 3, String(countOf('create_tag')));
check('annotated tag exists locally in all three', ['alpha', 'beta', 'gamma'].every(name => names(name).includes('v2.4.0')) && tags.list(repos.alpha).find(tag => tag.name === 'v2.4.0').annotated);
check('pushed to alpha and gamma remotes only', remoteNames('alpha').includes('v2.4.0') && remoteNames('gamma').includes('v2.4.0') && !remoteNames('beta').includes('v2.4.0'));
const results = await page.locator('.tag-dialog [aria-label=Results]').innerText();
check('one failure among three is shown, others succeed', results.includes('Tagged and pushed') && results.includes('Tagged, push failed') && (await page.locator('.tag-error').count()) === 1, results.replace(/\n/g, ' | ').slice(0, 160));
check('refs and trees were force-reloaded after the change', refCalls.some(entry => entry[0] !== 'tree' && entry.length) && refCalls.some(entry => entry[0] === 'tree'), JSON.stringify(refCalls.slice(-3)));
await shot('bulk-result');
await page.click('.tag-dialog footer .btn.dark');
await page.waitForTimeout(700);
check('focus returns to More after the dialog', await page.evaluate(() => document.activeElement?.classList.contains('more')), await page.evaluate(() => document.activeElement?.outerHTML.slice(0, 60)));

// Duplicate refusal: every repository already has v2.4.0.
await page.click('.fm-bar .more');
await page.click('[role=menuitem]:has-text("Tag…")');
await page.waitForSelector('.tag-dialog[open]');
await page.fill('.tag-dialog input.mono', 'v2.4.0');
await page.waitForSelector('.tag-badge.warn');
const dup = await dialogText();
check('duplicate name is refused in the preview', dup.includes('Refused: tag exists') && (await page.locator('.tag-dialog footer .btn.dark').isDisabled()), dup.replace(/\n/g, ' | ').slice(0, 140));
const createsBefore = countOf('create_tag');
await shot('duplicate-refused');
// Move: a new commit in alpha, then a confirmed move that shows old and new commit.
await page.click('.tag-dialog footer .btn:not(.dark)');
await page.waitForTimeout(500);
addCommit(repos.alpha, 'c.txt', 'Add c');
await page.evaluate(() => window.__emit?.('noop', null));
await page.click('.fm-bar .more');
await page.click('[role=menuitem]:has-text("Tag…")');
await page.waitForSelector('.tag-dialog[open]');
await page.fill('.tag-dialog input.mono', 'v2.4.0');
await page.fill('.tag-dialog textarea', 'Release 2.4.0 again');
await page.check('.tag-dialog .check:has-text("Move existing tags") input');
await page.check('.tag-dialog .check:has-text("Push after creating") input');
check('dialog never created a tag while refused', countOf('create_tag') === createsBefore);
await shot('move-review');
const oldAlpha = tags.list(repos.alpha).find(tag => tag.name === 'v2.4.0').commit;
await page.dblclick('.tag-dialog footer .btn.dark');
await page.waitForSelector('.confirm-dialog[open]');
const moveText = await page.locator('.confirm-dialog').innerText();
check('move confirmation shows old commit', moveText.includes(oldAlpha.slice(0, 8)) && moveText.includes('alpha') && moveText.includes('Move'), moveText.replace(/\n/g, ' ').slice(0, 160));
await shot('move-confirm');
await page.click('.confirm-dialog button:has-text("Cancel")');
await page.waitForTimeout(400);
check('double click opened one confirmation only', (await page.locator('.confirm-dialog[open]').count()) === 0);
check('cancelled move changes nothing', tags.list(repos.alpha).find(tag => tag.name === 'v2.4.0').commit === oldAlpha);
const movesBefore = countOf('create_tag');
await page.click('.tag-dialog footer .btn.dark');
await page.waitForSelector('.confirm-dialog[open]');
await page.click('.confirm-dialog .btn.danger');
await page.waitForSelector('.tag-lead:has-text("Tagged")');
check('confirmed move ran once per repository', countOf('create_tag') - movesBefore === 3, String(countOf('create_tag') - movesBefore));
await page.waitForTimeout(500);
const head = git(repos.alpha, 'rev-parse', 'HEAD').trim();
check('confirmed move retargets alpha locally and on the remote', tags.list(repos.alpha).find(tag => tag.name === 'v2.4.0').commit === head && git(remotes.alpha, 'rev-parse', 'v2.4.0^{commit}').trim() === head);
await shot('move-result');
await page.click('.tag-dialog footer .btn.dark');
await page.waitForTimeout(600);

// Drawer on alpha: tag list and chip on the rail.
await row('alpha').hover();
await page.click('button[aria-label="More actions for alpha"]');
await page.click('[role=menuitem]:has-text("History")');
await page.waitForSelector('.tag-section .tag-entry');
check('drawer lists alpha tags', (await page.locator('.tag-entry').count()) === 1 && (await page.locator('.tag-entry').innerText()).includes('v2.4.0'));
check('rail shows the tag at its commit', (await page.locator('.history-row:has(.history-tagref)').count()) === 1 && (await page.locator('.history-row:has(.history-tagref)').innerText()).includes('v2.4.0'));
await shot('drawer');
await page.click('.tag-section .tag-head .btn');
await page.waitForSelector('.tag-dialog[open]');
await page.fill('.tag-dialog input.mono', 'v2.5.0-rc');
await shot('drawer-new-tag');
await page.click('.tag-dialog footer .btn.dark');
await page.waitForSelector('.tag-lead:has-text("Tagged")');
await page.click('.tag-dialog footer .btn.dark');
await page.waitForSelector('.tag-entry:has-text("v2.5.0-rc")');
await page.waitForTimeout(700);
check('drawer refreshed with the lightweight tag and focus stayed in the drawer', (await page.locator('.tag-entry').count()) === 2 && await page.evaluate(() => !!document.activeElement?.closest('.history-drawer')), await page.evaluate(() => document.activeElement?.outerHTML.slice(0, 60)));
check('rails now show two tag chips', (await page.locator('.history-row:has(.history-tagref)').count()) >= 1 && (await page.locator('.history-tagref').count()) === 2);
await shot('drawer-two-tags');

// Delete from the drawer: local first, then the remote with its own confirmation.
await page.click('button[aria-label="Delete tag v2.5.0-rc"]');
await page.waitForSelector('.tag-dialog[open]');
check('delete dialog is prefilled with the tag', (await page.locator('.tag-dialog input.mono').first().inputValue()) === 'v2.5.0-rc');
await page.click('.tag-dialog footer .btn.danger:has-text("local")');
await page.waitForSelector('.tag-targets li:has-text("Deleted locally")');
check('local delete removed only the local tag', !names('alpha').includes('v2.5.0-rc') && names('alpha').includes('v2.4.0'));
await shot('delete-local-done');
await page.click('.tag-dialog footer .btn:not(.danger)');
await page.waitForTimeout(700);
check('focus falls back to New tag after deleting from the drawer', await page.evaluate(() => document.activeElement?.hasAttribute('data-tag-new')), await page.evaluate(() => document.activeElement?.outerHTML.slice(0, 80)));
await page.keyboard.press('Escape');
await page.waitForTimeout(600);

// Delete across the set: local first, then the separate remote confirmation.
await page.click('.fm-bar .more');
await page.click('[role=menuitem]:has-text("Delete tag…")');
await page.waitForSelector('.tag-dialog[open]');
await page.fill('.tag-dialog input.mono', 'v2.4.0');
await page.waitForTimeout(600);
await shot('delete-review');
await page.click('.tag-dialog footer .btn.danger:has-text("local")');
await page.waitForSelector('.tag-targets li:has-text("Deleted locally")');
await page.waitForTimeout(500);
check('local delete removed v2.4.0 everywhere but left the remotes', ['alpha', 'beta', 'gamma'].every(name => !names(name).includes('v2.4.0')) && remoteNames('alpha').includes('v2.4.0') && remoteNames('gamma').includes('v2.4.0'));
check('remote delete was not called by the local step', countOf('delete_remote_tag') === 0);
await shot('delete-local-set');
await page.click('.tag-dialog footer .btn.danger:has-text("remote")');
await page.waitForSelector('.confirm-dialog[open]');
const remoteText = await page.locator('.confirm-dialog').innerText();
check('remote confirmation names remote, repositories and the object each will delete', remoteText.includes('alpha (origin): object') && remoteText.includes('beta (origin): object') && remoteText.includes('gamma (origin): object'), remoteText.replace(/\n/g, ' ').slice(0, 160));
await shot('remote-confirm');
await page.click('.confirm-dialog button:has-text("Cancel")');
await page.waitForTimeout(400);
check('cancel keeps the remote tags', countOf('delete_remote_tag') === 0 && remoteNames('alpha').includes('v2.4.0'));
git(remotes.gamma, 'tag', '-f', '-a', '-m', 'moved on the remote', 'v2.4.0', 'main');
await page.click('.tag-dialog footer .btn.danger:has-text("remote")');
await page.waitForSelector('.confirm-dialog[open]');
await page.click('.confirm-dialog .btn.danger');
await page.waitForSelector('.tag-targets li:has-text("Deleted on remote")');
await page.waitForTimeout(600);
await shot('remote-result');
check('remote delete is leased: alpha removed, gamma kept because its remote tag moved, beta failed', !remoteNames('alpha').includes('v2.4.0') && remoteNames('gamma').includes('v2.4.0') && remoteNames('gamma').includes('v1.0.0') && (await page.locator('.tag-targets li:has-text("Remote kept")').count()) === 2, remoteNames('gamma').join());
check('every submitted remote delete carried the expected object', tags.calls.filter(call => call.command === 'delete_remote_tag').every(call => /^[0-9a-f]{40}$/.test(call.expected)));
await page.click('.tag-dialog footer .btn:not(.danger)');
await page.waitForTimeout(600);
await page.click('.fm-bar .more');
await page.click('[role=menuitem]:has-text("Delete tag…")');
await page.waitForSelector('.tag-dialog[open]');
await page.fill('.tag-dialog input.mono', 'v2.4.0');
await page.waitForTimeout(500);
await page.click('.tag-dialog footer .btn.danger:has-text("remote")');
await page.waitForSelector('.confirm-dialog[open]');
const unknownText = await page.locator('.confirm-dialog').innerText();
check('repositories without a known object are listed as refused', (unknownText.match(/unknown — will be refused/g) ?? []).length === 3, unknownText.replace(/\n/g, ' ').slice(0, 200));
await shot('remote-confirm-unknown');
const callsBefore = countOf('delete_remote_tag');
await page.click('.confirm-dialog .btn.danger');
await page.waitForSelector('.tag-targets li:has-text("Not submitted")');
check('nothing was submitted for unknown objects', countOf('delete_remote_tag') === callsBefore && (await page.locator('.tag-targets li:has-text("Not submitted")').count()) === 3);
await shot('remote-not-submitted');
await page.click('.tag-dialog footer .btn:not(.danger)');
await page.waitForTimeout(500);

for (let index = 0; index < 60; index += 1) git(repos.alpha, 'tag', `v3.${index}`);
await row('alpha').hover();
await page.click('button[aria-label="More actions for alpha"]');
await page.click('[role=menuitem]:has-text("History")');
await page.waitForSelector('.tag-entry');
check('drawer lists newest version first and caps at 50', (await page.locator('.tag-entry').count()) === 50 && (await page.locator('.tag-entry .tag-title').first().innerText()) === 'v3.59');
await page.click('.tag-more');
check('Show all lists every tag', (await page.locator('.tag-entry').count()) === 60);
await shot('drawer-many-tags');
console.log(unknown.size ? `UNMOCKED ${[...unknown].join(',')}` : 'no unmocked commands');
await browser.close();
