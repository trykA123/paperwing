// Pull request column, open dialog, bulk open and lazy loading in Helium with mocked Tauri commands over 800 rows.
// Usage: PLAYWRIGHT_DIR=<dir with playwright-core> node ui-28.mjs <url> <shots-dir> [light|dark] [width] [compact|comfortable] [full|shots]
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { hostOf, lastCommit, makePulls, remoteTree, statusFor } from './pulls-backend.mjs';

const [url, shots, theme = 'light', width = '1440', density = 'compact', mode = 'full'] = process.argv.slice(2);
const { chromium } = await import(`${process.env.PLAYWRIGHT_DIR}/index.mjs`);
const caps = Object.fromEntries(['readCompare', 'edit', 'copy', 'recovery', 'trash'].map(name => [name, { supported: true, reason: null }]));
const COUNT = 800;
const items = Array.from({ length: COUNT }, (_, index) => ({ id: `repo-${index}`, repoId: `fixture:${index}`, url: `https://${hostOf(index)}/org/repo-${index}.git`, org: 'org', name: `repo-${index}`, ref: { type: 'branch', name: `feature/${index}` }, on: false }));
const workspace = { sets: [{ id: 'fix', name: 'Login rollout', items }], activeSet: 'fix', root: '/work', theme, pageSize: 'all', density };
const pulls = makePulls();
const unknown = new Set();
const pushes = [];

const browser = await chromium.launch({ executablePath: '/opt/helium-browser-bin/helium', headless: true, args: ['--no-sandbox'] });
const page = await (await browser.newContext({ viewport: { width: Number(width), height: 900 }, colorScheme: theme })).newPage();
page.on('pageerror', error => console.log('PAGE ERROR', error.message));
page.on('console', message => { if (['error', 'warning'].includes(message.type())) console.log('CONSOLE', message.type(), message.text()); });

const handle = async (cmd, a) => {
  switch (cmd) {
    case 'load_settings': return { sources: [{ id: 'fixture', name: 'Fixture', kind: 'manual', host: '', orgs: [], urls: [] }], workspace };
    case 'platform_info': return { platform: 'linux', separator: '/', capabilities: caps, credentials: { backend: 'unsupported', persistent: false, supported: false, reason: null } };
    case 'probe_root': return { root: a.root, valid: true, reason: null, identity: 'fixture', casePolicy: 'sensitive', capabilities: caps };
    case 'path_identities': return a.paths.map(path => ({ path, identity: path, exists: true, reason: null }));
    case 'paths_exist': return a.paths.map(() => true);
    case 'local_status': return a.paths.map(statusFor);
    case 'pull_for_branch': return pulls.pullForBranch(a.path, a.branch);
    case 'open_pull_request': return pulls.openPullRequest(a.path, a.request);
    case 'push_branch': pushes.push(a.path); return { remote: 'origin', branch: 'x', upstreamSet: true };
    case 'repository_tree': return remoteTree();
    case 'repository_history': return lastCommit(a.path);
    case 'plugin:opener|open_url': pulls.browser = [...(pulls.browser ?? []), a.url]; return null;
    case 'activity_snapshot': case 'get_refs_many': return [];
    case 'list_repos': return { repos: [], fetchedAt: 0, errors: [] };
    case 'save_settings': case 'list_cached_repos': case 'plugin:window|set_theme': return null;
    case 'source_revision': return 0;
    case 'credential_status': return { sourceId: a.sourceId, backend: 'unsupported', state: 'unavailable', revision: 0, reason: null };
    default: unknown.add(cmd); return null;
  }
};
await page.exposeFunction('__backend', async (cmd, a) => {
  try { return { value: await handle(cmd, a) }; } catch (error) { return { error: error instanceof Error ? error.message : error }; }
});
await page.addInitScript(() => {
  let next = 1;
  const listeners = {};
  window.__TAURI_INTERNALS__ = {
    transformCallback(fn) { const id = next++; window['_' + id] = fn; return id; },
    async invoke(cmd, args) {
      if (cmd === 'plugin:event|listen') { (listeners[args.event] ??= []).push(args.handler); return listeners[args.event].length; }
      if (cmd === 'plugin:event|unlisten') return null;
      const reply = await window.__backend(cmd, args ?? {});
      if ('error' in reply) throw reply.error;
      return reply.value;
    },
    convertFileSrc: path => path,
    metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main', windowLabel: 'main' } },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener(event, id) { const list = listeners[event]; if (list) list[id - 1] = null; } };
});

mkdirSync(shots, { recursive: true });
const tag = `${theme}-${width}-${density}`;
const shot = async name => { await page.waitForTimeout(450); return page.screenshot({ path: join(shots, `28-${tag}-${name}.png`) }); };
const check = (label, ok, detail = '') => { console.log(`${ok ? 'PASS' : 'FAIL'} ${label} ${detail}`); if (!ok) process.exitCode = 1; };
const settle = () => page.waitForTimeout(1200);
// SyncRails' sr-only spans escape the table clip, so focus changes can scroll <main>; reset it.
const scrollTo = top => page.evaluate(top => { document.querySelector('main').scrollTop = 0; document.querySelector('.fm-table .vbox').scrollTop = top; }, top);
const row = index => page.locator(`.fm-row[data-id="repo-${index}"]`);
const rowH = density === 'compact' ? 40 : 56;

await page.goto(url);
await page.waitForTimeout(5500);
pulls.calls.length = 0;
await page.reload();
await page.waitForSelector('.fm-row[data-id="repo-0"]');
await settle();
const first = pulls.calls.length;
const mounted = await page.locator('.fm-table .fm-row[data-id]').count();
check(`initial load asks only for visible rows (${first} calls, ${mounted} rows mounted, ${COUNT} in set)`, first > 0 && first <= mounted && first < 60);
const box = await page.evaluate(() => ({ wrap: document.querySelector('.fm-wrap').clientWidth, row: document.querySelector('.fm-table .fm-row[data-id]').scrollWidth, view: document.querySelector('.fm-table .vbox').clientWidth }));
console.log(`widths wrap=${box.wrap} view=${box.view} rowScroll=${box.row}`);
await shot('table-top');
await page.click('button[aria-label="Toggle sidebar"]');
await page.waitForTimeout(400);
const wide = await page.evaluate(() => ({ wrap: document.querySelector('.fm-wrap').clientWidth, text: document.querySelectorAll('.pull-text').length && getComputedStyle(document.querySelector('.pull-text')).display }));
console.log(`panels hidden: wrap=${wide.wrap} pull-text display=${wide.text}`);
await shot('table-wide');
await page.click('button[aria-label="Toggle sidebar"]');
await page.waitForTimeout(400);

await scrollTo(rowH * 400);
await settle();
const afterScroll = pulls.calls.length;
check(`scrolling to the middle adds only the rows shown (${afterScroll - first} new calls)`, afterScroll > first && afterScroll - first < 60);
await scrollTo(0);
await settle();
check('scrolling back re-requests nothing (session cache)', pulls.calls.length === afterScroll, String(pulls.calls.length));
check('no path asked twice', new Set(pulls.calls).size === pulls.calls.length, pulls.calls.filter((path, at) => pulls.calls.indexOf(path) !== at).join());

const text = async index => (await row(index).locator('.fm-pull').innerText()).replace(/\s+/g, ' ').trim();
const texts = await Promise.all([0, 1, 2, 3, 4, 5, 6, 7].map(text));
console.log('cells', JSON.stringify(texts));
check('open/approved/checks row', /#100/.test(texts[0]) && /Open/.test(texts[0]));
check('draft, merged and closed rows', /Draft/.test(texts[1]) && /Merged/.test(texts[2]) && /Closed/.test(texts[3]));
check('GHES host without a source says Add a source', /Add a source/.test(texts[7]) && (await row(7).locator('.pull-part').getAttribute('title')).includes('ghe.example.test'));
check('branch without a pull request says None', /None/.test(texts[6]));

if (mode === 'shots') { console.log(unknown.size ? `UNMOCKED ${[...unknown].join(',')}` : 'no unmocked commands'); await browser.close(); process.exit(); }

await row(0).locator('.pull-chip').click();
await page.waitForTimeout(300);
check('chip opens the pull request URL in the browser', (pulls.browser ?? []).includes('https://example.test/org/repo-0/pull/100'), JSON.stringify(pulls.browser));

// Row menu, open dialog on a branch without a pull request, repository-default base and last-commit title.
await row(6).hover();
await page.click('button[aria-label="More actions for repo-6"]');
const withPull = async index => { await row(index).hover(); await page.click(`button[aria-label="More actions for repo-${index}"]`); return page.locator('[role=menuitem]:has-text("Open pull request")'); };
await page.keyboard.press('Escape');
const live = await withPull(0);
check('menu disables Open pull request when one is open, with the number', (await live.isDisabled()) && ((await live.getAttribute('title')) ?? '').includes('#100'), await live.getAttribute('title'));
await page.keyboard.press('Escape');
await (await withPull(6)).click();
await page.waitForSelector('.pull-dialog[open]');
await page.waitForFunction(() => document.querySelector('.pull-dialog input[required]')?.value);
check('title defaults to the last commit subject', (await page.inputValue('.pull-dialog input[required]')) === 'Wire up login for repo 6');
check('base defaults to the remote HEAD', ((await page.locator('.pull-dialog .select, .pull-dialog [aria-label="Base branch"]').first().innerText()) ?? '').includes('main'));
check('dialog names the target repository', (await page.locator('.pull-target').innerText()).includes('org/repo-6'));
await shot('open-dialog');
await page.fill('.pull-dialog textarea', 'Adds the login flow.');
await page.uncheck('.pull-dialog .check input >> nth=0');
await page.click('.pull-dialog button[type=submit]');
await page.waitForSelector('.pull-done');
const sent = pulls.opened.at(-1);
check('request carries head, base, title, body and the draft flag', sent.request.head === 'feature/6' && sent.request.base === 'main' && sent.request.body === 'Adds the login flow.' && sent.request.draft === false, JSON.stringify(sent.request));
await shot('open-dialog-done');
await page.click('.pull-dialog footer .btn:not(.dark)');
await page.waitForTimeout(500);
check('focus returns to the row menu button', await page.evaluate(() => document.activeElement?.getAttribute('aria-label') === 'More actions for repo-6'), await page.evaluate(() => document.activeElement?.outerHTML.slice(0, 80)));
await settle();
check('the row refreshed and shows the new pull request chip', /#506/.test(await text(6)) && /Open/.test(await text(6)), await text(6));

// Unpublished branch: submit stays blocked until Push first.
await (await withPull(9)).click();
await page.waitForSelector('.pull-dialog[open]');
check('unpublished branch blocks submit and offers Push first', (await page.locator('.pull-dialog button[type=submit]').isDisabled()) && (await page.locator('.pull-dialog .check:has-text("Push first")').count()) === 1);
await shot('open-dialog-unpublished');
await page.keyboard.press('Escape');
await page.waitForTimeout(400);

// Drawer.
await row(4).locator('.fm-sub').click();
await page.waitForSelector('.pull-section .pull-chip');
check('drawer shows the pull request with base and target', (await page.locator('.pull-section').innerText()).includes('org/repo-4'));
await shot('drawer');
await page.keyboard.press('Escape');
await page.waitForSelector('.history-drawer', { state: 'detached' });

// Bulk open: 11 opens, 9 is unpublished and skipped until Push first, 10 fails at GitHub.
for (const index of [11, 9, 10]) await row(index).locator('input[type=checkbox]').check();
await page.waitForSelector('.fm-bar.on');
await page.click('.fm-bar .more');
await page.click('[role=menuitem]:has-text("Open pull requests")');
await page.waitForSelector('.pull-bulk[open] .pull-table');
await page.waitForFunction(() => document.querySelectorAll('.pull-bulk tbody tr').length === 3);
const preview = (await page.locator('.pull-bulk tbody').innerText()).replace(/\s+/g, ' ');
check('preview lists three repositories and skips the unpublished one', preview.includes('repo-9') && /Skipped: .*not on the remote/.test(preview) && !(await page.locator('.pull-bulk button:has-text("Open 3")').count()), preview);
await shot('bulk-preview');
await page.click('.pull-bulk .check:has-text("Push first") input');
const pushBefore = pushes.length;
check('Push first changes the plan without pushing yet', pushes.length === 0 && /pushed first/.test((await page.locator('.pull-bulk tbody').innerText()).replace(/\s+/g, ' ')));
await shot('bulk-push-first');
await page.click('.pull-bulk footer .btn.dark');
await page.waitForSelector('.pull-bulk footer .btn.dark:not([disabled]):has-text("Done")');
const result = (await page.locator('.pull-bulk tbody').innerText()).replace(/\s+/g, ' ');
check('results: two opened, one failed with the GitHub message', (result.match(/#5\d\d/g) ?? []).length === 2 && /Failed: .*already exists/.test(result), result);
check('only the unpublished branch was pushed', pushes.length === pushBefore + 1 && pushes[0] === '/work/repo-9', JSON.stringify(pushes));
check('requests went out in order, drafts on', pulls.opened.slice(-3).map(entry => entry.path.split('/').pop()).join() === 'repo-9,repo-10,repo-11' && pulls.opened.slice(-3).every(entry => entry.request.draft === true));
await shot('bulk-result');
await page.click('.pull-bulk footer .btn.dark');
await page.waitForTimeout(500);
check('focus returns to the More button', await page.evaluate(() => document.activeElement?.classList.contains('more')), await page.evaluate(() => document.activeElement?.outerHTML.slice(0, 60)));

// Rate limit: the batch stops, shows the reset time (Europe/Bucharest) and stops asking.
pulls.limitAfter = pulls.calls.length;
pulls.resetAt = new Date(Date.now() + 9000).toISOString();
await scrollTo(rowH * 600);
const noticed = await page.waitForSelector('.notice:has-text("rate limit")', { timeout: 5000 }).then(handle => handle.innerText(), () => '');
await settle();
const limited = pulls.calls.length;
await scrollTo(rowH * 700);
await settle();
await scrollTo(rowH * 300);
await settle();
check(`after the limit no more requests go out (${pulls.calls.length - pulls.limitAfter} past the limit)`, pulls.calls.length - limited === 0 && limited - pulls.limitAfter <= 4, `${limited} then ${pulls.calls.length}`);
check('a notice names the reset time', /rate limit/i.test(noticed) && /\d{1,2}:\d{2}/.test(noticed), noticed.replace(/\s+/g, ' '));
const paused = await page.locator('.pull-part:has-text("Paused")').count();
check('rows still waiting show Paused with the reset time', paused > 0, String(paused));
await page.evaluate(() => { document.querySelector('main').scrollTop = 0; });
await shot('rate-limited');
const beforeReset = pulls.calls.length;
pulls.limitAfter = Infinity;
await page.waitForFunction(() => !document.querySelector('.pull-part.warn'), null, { timeout: 15000 }).catch(() => {});
await settle();
check('after the reset the waiting rows load without scrolling and Paused disappears', pulls.calls.length > beforeReset && (await page.locator('.pull-part:has-text("Paused")').count()) === 0, `${beforeReset} -> ${pulls.calls.length}`);

// Bulk with a rate limit: rows that could not be checked are not submitted.
await page.click('.fm-bar .x');
pulls.limitAfter = pulls.calls.length;
pulls.resetAt = new Date(Date.now() + 30 * 60_000).toISOString();
await scrollTo(rowH * 750);
await page.waitForSelector('.pull-part:has-text("Paused")', { timeout: 8000 });
const waiting = page.locator('.fm-row:has(.pull-part:has-text("Paused")) input[type=checkbox]');
for (let at = 0; at < 3; at += 1) await waiting.nth(at).check();
const opensBefore = pulls.opened.length;
await page.click('.fm-bar .more');
await page.click('[role=menuitem]:has-text("Open pull requests")');
await page.waitForSelector('.pull-bulk[open] .pull-table');
const unchecked = (await page.locator('.pull-bulk tbody').innerText()).match(/Not checked \(rate limited\)/g) ?? [];
check('rate limit during the bulk check marks the rows Not checked and nothing can be submitted', unchecked.length === 3 && (await page.locator('.pull-bulk footer .btn.dark').isDisabled()), String(unchecked.length));
await shot('bulk-not-checked');
await page.keyboard.press('Escape');
check('nothing was opened for unchecked rows', pulls.opened.length === opensBefore);
check('rows carry no role=status or role=alert', (await page.locator('.fm-table [role=status], .fm-table [role=alert]').count()) === 0);
check('<main> no longer scrolls from the table', await page.evaluate(() => document.querySelector('main').scrollHeight <= document.querySelector('main').clientHeight + 300));
console.log(`total pull_for_branch calls: ${pulls.calls.length}`);
console.log(unknown.size ? `UNMOCKED ${[...unknown].join(',')}` : 'no unmocked commands');
await browser.close();
