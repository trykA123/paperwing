// Drives the Vite dev server in Helium with a mocked Tauri invoke layer backed by real Git fixtures.
// Usage: PLAYWRIGHT_DIR=<dir with playwright> node ui-39.mjs <url> <shots-dir> [light|dark] [width]
import { strict as assert } from 'node:assert';
import { mkdtempSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { calls, deleteMerged, localStatus, mergedBranches } from './backend.mjs';
import { openSet } from './shell-nav.mjs';
import { git, makeCapRepo, makeDivergentRepo, makeRepo, moveTip } from './fixture.mjs';
import { SearchJobs } from './search-jobs.mjs';

const [url, shots, theme = 'light', width = '1440'] = process.argv.slice(2);
const { chromium } = await import(`${process.env.PLAYWRIGHT_DIR}/index.mjs`);
const root = mkdtempSync(join(tmpdir(), 'skein-ui39-'));
const repos = { alpha: join(root, 'alpha'), beta: join(root, 'beta repo'), gamma: join(root, 'gamma') };
const remote = join(root, 'alpha-remote.git');
makeRepo(repos.alpha, { remote }); makeRepo(repos.beta, { todo: 2 }); makeRepo(repos.gamma, { todo: 4 });
const caps = Object.fromEntries(['readCompare', 'edit', 'copy', 'recovery', 'trash'].map(name => [name, { supported: true, reason: null }]));
const delta = join(root, 'delta repo'), deltaRemote = join(root, 'delta-remote.git');
makeDivergentRepo(delta, deltaRemote);
const capPaths = Array.from({ length: 13 }, (_, index) => join(root, 'caps', `c${String(index + 1).padStart(2, '0')}`));
capPaths.forEach(path => makeCapRepo(path));
const itemsOf = paths => paths.map(path => ({ id: path.split('/').pop(), repoId: 'fixture:' + path, url: path, org: 'fixture', name: path.split('/').pop(), ref: { type: 'branch', name: 'main' }, on: false, path }));
const workspace = { sets: [{ id: 'fix', name: 'Fixture set', items: itemsOf(Object.values(repos)) }, { id: 'div', name: 'Divergent set', items: itemsOf([delta]) }, { id: 'caps', name: 'Caps set', items: itemsOf(capPaths) }], activeSet: 'fix', root, theme, pageSize: 'all', shell: { version: 1, sidebarWidth: 250, sidebarVisible: Number(width) >= 700, rightVisible: Number(width) >= 700, section: 'sets' } };
const source = { id: 'fixture', name: 'Fixture', kind: 'manual', host: '', orgs: [], urls: [] };

const browser = await chromium.launch({ executablePath: '/opt/helium-browser-bin/helium', headless: true, args: ['--no-sandbox'] });
const context = await browser.newContext({ viewport: { width: Number(width), height: 900 }, permissions: ['clipboard-read', 'clipboard-write'] });
const page = await context.newPage();
page.on('pageerror', error => console.log('PAGE ERROR', error.message));
page.on('console', message => { if (['error', 'warning'].includes(message.type())) console.log('CONSOLE', message.type(), message.text()); });
const jobs = new SearchJobs((event, payload) => page.evaluate(([name, body]) => window.__emit(name, body), [event, payload]).catch(() => {}));
const unknown = new Set();

await page.exposeFunction('__backend', async (cmd, a) => {
  switch (cmd) {
    case 'load_settings': return { sources: [source], workspace };
    case 'platform_info': return { platform: 'linux', separator: '/', capabilities: caps, credentials: { backend: 'unsupported', persistent: false, supported: false, reason: null } };
    case 'probe_root': return { root: a.root, valid: true, reason: null, identity: 'fixture', casePolicy: 'sensitive', capabilities: caps };
    case 'path_identities': return a.paths.map(path => ({ path, identity: path, exists: true, reason: null }));
    case 'paths_exist': return a.paths.map(() => true);
    case 'local_status': return localStatus(a.paths);
    case 'activity_snapshot': case 'get_refs_many': return [];
    case 'list_repos': return { repos: [], fetchedAt: 0, errors: [] };
    case 'merged_branches': return mergedBranches(a.path);
    case 'delete_merged_branches': return deleteMerged(a.path, a.names, a.expected, a.base);
    case 'search_capabilities': return { perl: true };
    case 'search_start': return jobs.start(a.request);
    case 'search_cancel': return jobs.cancel(a.id);
    case 'search_cancel_all': return jobs.cancelAll();
    case 'save_settings': case 'pull_for_branch': case 'list_cached_repos': case 'plugin:window|set_theme': return null;
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
  window.__listeners = event => (listeners[event] ?? []).filter(Boolean).length;
});

mkdirSync(shots, { recursive: true });
const shot = async name => { await page.waitForTimeout(400); return page.screenshot({ path: join(shots, `${theme}-${width}-${name}.png`) }); };
const check = (label, ok, detail = '') => { console.log(`${ok ? 'PASS' : 'FAIL'} ${label} ${detail}`); if (!ok) process.exitCode = 1; };
const branches = dir => git(dir, 'for-each-ref', '--format=%(refname:short)', 'refs/heads').split('\n').filter(Boolean);

await page.goto(url);
await page.waitForTimeout(5500);
await page.waitForSelector('.fm-row[data-id]');
await openSet(page, 'Fixture set', width);
await page.waitForSelector('.rf-setbar');
await page.waitForTimeout(800);
await shot('set');

// 1. Cleanup from the row menu on a repository with a remote.
await page.hover('.fm-row[data-id="alpha"]');
await page.click('button[aria-label="More actions for alpha"]');
await page.click('[role=menuitem]:has-text("Clean up merged branches")');
await page.waitForSelector('.cleanup-dialog[open] .cleanup-row');
await page.waitForTimeout(400);
const offered = await page.$$eval('.cleanup-list:first-of-type .cleanup-row:not(.blocked) .cleanup-name', nodes => nodes.map(node => node.textContent));
const preselected = await page.$$eval('.cleanup-row:not(.blocked) input:checked + .cleanup-name', nodes => nodes.map(node => node.textContent));
check('selectable local branches', offered.join() === 'feature/done,feature/two,gone', offered.join());
check('preselection skips upstream-gone', preselected.join() === 'feature/done,feature/two', preselected.join());
const kept = await page.$$eval('.cleanup-blocked .cleanup-row', nodes => nodes.map(node => node.textContent));
check('kept rows explain themselves', kept.some(text => text.includes('wt-branch') && text.includes('worktree')) && kept.some(text => text.includes('wip') && text.includes('Not merged')), kept.join(' | '));
check('current branch never listed', !(await page.locator('.cleanup-dialog').innerText()).match(/\bdev\b/));
check('squash note shown', (await page.locator('.cleanup-dialog').innerText()).includes('Squash-merged branches are not detected'));
await page.click('.cleanup-blocked summary');
await shot('cleanup-preview');
await page.click('.cleanup-dialog footer .btn.dark');
await page.waitForSelector('.cleanup-repo [role=status]:has-text("2 deleted")');
check('selected merged branches deleted, others kept', branches(repos.alpha).join() === 'dev,gone,main,wip,wt-branch', branches(repos.alpha).join());
await shot('cleanup-done');

// 2. No remote section.
check('no remote tab, remote list or remote delete button', (await page.locator('.cleanup-tabs, .cleanup-list[aria-label^="Branches on"], .cleanup-dialog .btn.danger').count()) === 0 && !(await page.locator('.cleanup-dialog').innerText()).includes('emote'));
check('remote branch kept', execFileSync('git', ['ls-remote', '--heads', remote], { encoding: 'utf8' }).includes('old-remote'));
check('no remote delete command was called', calls.every(call => call.command !== 'delete_remote_branches'));
await page.click('.cleanup-dialog footer .btn:not(.danger):not(.dark)');
await page.waitForSelector('.cleanup-dialog', { state: 'detached' });

// 3. Set-wide cleanup with one expected failure.
await page.check('input[aria-label="Select all repositories on this page"]');
await page.click('.fm-bar .more');
await page.click('[role=menuitem]:has-text("Clean up merged branches")');
await page.waitForSelector('.cleanup-dialog[open] .cleanup-repo h3');
await page.waitForFunction(() => document.querySelectorAll('.cleanup-repo .cleanup-list').length === 3);
moveTip(repos.gamma, 'feature/two');
await shot('set-cleanup-preview');
await page.click('.cleanup-dialog footer .btn.dark');
await page.waitForSelector('.cleanup-repo [role=status].err, .cleanup-repo .banner.err');
await page.waitForTimeout(500);
check('beta cleaned', branches(repos.beta).join() === 'dev,main,wip,wt-branch', branches(repos.beta).join());
check('gamma keeps the moved branch', branches(repos.gamma).includes('feature/two') && !branches(repos.gamma).includes('feature/done'), branches(repos.gamma).join());
await shot('set-cleanup-result');
await page.keyboard.press('Escape');
await page.waitForSelector('.cleanup-dialog', { state: 'detached' });

// 4. Code search.
const frames = async run => {
  await page.evaluate(() => {
    window.__gaps = []; let last = performance.now();
    const tick = now => { window.__gaps.push(now - last); last = now; window.__raf = requestAnimationFrame(tick); };
    window.__raf = requestAnimationFrame(tick);
  });
  await run();
  const gaps = await page.evaluate(() => { cancelAnimationFrame(window.__raf); return window.__gaps.slice(2); });
  return { count: gaps.length, worst: Math.max(...gaps), over: gaps.filter(gap => gap > 50).length };
};
const search = () => page.click('.cs-bar button[type=submit]');
const waitDone = text => page.waitForSelector(`.cs-status:has-text("${text}")`, { timeout: 30000 });

await page.keyboard.press('Control+Shift+F');
await page.waitForSelector('.code-search');
check('right panel hidden for code search', (await page.locator('.shell-right').getAttribute('aria-hidden')) === 'true');
await page.fill('.cs-pattern input', 'TODO');
await search();
await waitDone('matches in 3 repositories');
const kinds = await page.$$eval('.cs-list .vrow > div', nodes => nodes.map(node => node.className.replace('cs-row ', '')));
check('grouped by repository then file', kinds.filter(kind => kind.startsWith('cs-repo')).length === 3 && kinds.some(kind => kind.startsWith('cs-file')));
check('matched text is highlighted', (await page.$$eval('.cs-match mark', nodes => nodes.map(node => node.textContent))).every(text => text.toUpperCase() === 'TODO'));
await shot('search-results');
await page.fill('.cs-paths input', 'src/*.ts');
await search();
await waitDone('matches in 3 repositories');
check('path filter excludes README', !(await page.locator('.cs-list').innerText()).includes('README.md'));
await page.click('.cs-match >> nth=0');
await page.waitForSelector('.notices :text("Copied")');
check('click copies the file path', (await page.evaluate(() => navigator.clipboard.readText())).includes(root));
await shot('search-copied');
await page.fill('.cs-paths input', '');

const one = await frames(async () => { await page.fill('.cs-pattern input', 'bigone'); await search(); await waitDone('matches in 3 repositories'); });
check('one event with 2000 matches stays smooth', one.over <= 1, `frames=${one.count} worst=${one.worst.toFixed(0)}ms over50=${one.over}`);
const live = await page.$$eval('[role=status]', nodes => nodes.map(node => node.textContent.trim()).filter(Boolean));
check('live region announces only the final count', live.some(text => /^\d+ matches in 3 repositories$/.test(text)) && live.every(text => !text.includes('Searching,')), live.join(' | '));

// Four-search limit.
for (let count = 0; count < 4; count++) jobs.start({ pattern: 'bighold', repos: [{ path: repos.alpha }] });
await page.fill('.cs-pattern input', 'TODO');
await search();
await page.waitForSelector('.banner.err:has-text("Too many searches")');
await shot('search-limit');
await page.click('button:has-text("Stop all searches and retry")');
await waitDone('matches in 3 repositories');
check('limit error offers a way out', true);
await page.click('.shell-tab.on .tab-close');
await page.waitForTimeout(400);
check('closing the tab leaves no running job', jobs.running.size === 0, jobs.log.slice(-3).join(' '));
check('closing the tab removes the listeners', (await page.evaluate(() => window.__listeners('search-matches'))) === 0);

// 5. Divergent bases: local master, remote main and master, no origin/HEAD.
await openSet(page, 'Divergent set', width);
await page.waitForSelector('.fm-row[data-id="delta repo"]');
await page.hover('.fm-row[data-id="delta repo"]');
await page.click('button[aria-label="More actions for delta repo"]');
await page.click('[role=menuitem]:has-text("Clean up merged branches")');
await page.waitForSelector('.cleanup-dialog[open] .cleanup-row');
const localOffered = await page.$$eval('.cleanup-row:not(.blocked) .cleanup-name', nodes => nodes.map(node => node.textContent));
check('local base is master', localOffered.join() === 'topic', localOffered.join());
check('remote branch kept on the divergent remote', execFileSync('git', ['ls-remote', '--heads', deltaRemote], { encoding: 'utf8' }).includes('rel-old'));
await page.waitForTimeout(500);
check('only one confirmation was queued', (await page.$$('.confirm-dialog')).length === 0);
await shot('divergent');
await page.click('.cleanup-dialog footer .btn:not(.danger):not(.dark)');
await page.waitForSelector('.cleanup-dialog', { state: 'detached' });

// 6. Caps, failed and skipped repositories, and 10000 matches.
await openSet(page, 'Caps set', width);
await page.waitForSelector('.fm-row[data-id="c01"]');
await page.waitForTimeout(1500);
await page.keyboard.press('Control+Shift+F');
await page.waitForSelector('.code-search');
await page.fill('.cs-pattern input', 'TODO');
await page.click('.cs-scope summary');
await page.fill('input[aria-label="Ref for c02"]', 'nosuchref');
await search();
await waitDone('matches in 13 repositories');
const states = await page.evaluate(async () => {
  const box = document.querySelector('.cs-list .vbox'), seen = new Map();
  for (let top = 0; top <= box.scrollHeight; top += 400) {
    box.scrollTop = top;
    await new Promise(resolve => requestAnimationFrame(resolve));
    for (const row of box.querySelectorAll('.cs-repo')) seen.set(row.querySelector('b').textContent, row.querySelector('.cs-state').textContent);
  }
  box.scrollTop = 0;
  return [...seen.values()];
});
const count = label => states.filter(state => state === label).length;
check('per-repository cap, overall cap, failed and skipped states', count('failed') === 1 && count('skipped') === 1 && count('truncated') === 11, states.join());
check('cap notice is shown', await page.locator('.banner.warn:has-text("result limit")').isVisible());
check('default overall cap is 2000', (await page.locator('.cs-status').innerText()).includes('2000 matches'), await page.locator('.cs-status').innerText());
await shot('search-caps');
await page.fill('input[aria-label="Ref for c02"]', '');
const big = await frames(async () => {
  await page.fill('.cs-pattern input', 'bigtest');
  await search();
  await page.waitForSelector('.cs-status:has-text("Searching")');
  for (let step = 0; step < 5; step++) { await page.evaluate(top => { document.querySelector('.cs-list .vbox').scrollTop = top; }, step * 60000); await page.waitForTimeout(300); }
  await waitDone('10000 matches');
});
check('10000 matches stream without long frames', big.over <= 2, `frames=${big.count} worst=${big.worst.toFixed(0)}ms over50=${big.over}`);
check('only visible rows are mounted', (await page.$$eval('.cs-list .vrow', nodes => nodes.length)) < 120);
await shot('search-big-done');

await search();
await page.waitForSelector('.cs-bar button.danger');
await page.waitForTimeout(500);
await shot('search-streaming');
await page.click('.cs-bar button.danger');
await page.waitForSelector('.cs-status:has-text("Search cancelled")');
check('stop cancels the job', jobs.log.some(entry => entry.startsWith('cancel')), jobs.log.join(' '));

await search();
await page.waitForSelector('.cs-bar button.danger');
await page.waitForTimeout(400);
const before = jobs.running.size;
await page.click('.shell-tab.on .tab-close');
await page.waitForTimeout(800);
check('closing a running search cancels it', before === 1 && jobs.running.size === 0, `before=${before} after=${jobs.running.size}`);

check('no unmocked commands', unknown.size === 0, [...unknown].join());
await browser.close();
