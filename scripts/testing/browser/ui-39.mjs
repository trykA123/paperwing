// Drives the Vite dev server in Helium with a mocked Tauri invoke layer backed by real Git fixtures.
// Usage: PLAYWRIGHT_DIR=<dir with playwright> node ui-39.mjs <url> <shots-dir> [light|dark] [width]
import { strict as assert } from 'node:assert';
import { mkdtempSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { deleteMerged, deleteRemote, localStatus, mergedBranches } from './backend.mjs';
import { git, makeRepo, moveTip } from './fixture.mjs';
import { SearchJobs } from './search-jobs.mjs';

const [url, shots, theme = 'light', width = '1440'] = process.argv.slice(2);
const { chromium } = await import(`${process.env.PLAYWRIGHT_DIR}/index.mjs`);
const root = mkdtempSync(join(tmpdir(), 'skein-ui39-'));
const repos = { alpha: join(root, 'alpha'), beta: join(root, 'beta repo'), gamma: join(root, 'gamma') };
const remote = join(root, 'alpha-remote.git');
makeRepo(repos.alpha, { remote }); makeRepo(repos.beta, { todo: 2 }); makeRepo(repos.gamma, { todo: 4 });
const caps = Object.fromEntries(['readCompare', 'edit', 'copy', 'recovery', 'trash'].map(name => [name, { supported: true, reason: null }]));
const items = Object.entries(repos).map(([name, path]) => ({ id: name, repoId: 'fixture:' + name, url: path, org: 'fixture', name: path.split('/').pop(), ref: { type: 'branch', name: 'main' }, on: false, path }));
const workspace = { sets: [{ id: 'fix', name: 'Fixture set', items }], activeSet: 'fix', root, theme, pageSize: 'all' };
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
    case 'delete_merged_branches': return deleteMerged(a.path, a.names, a.expected);
    case 'delete_remote_branches': return deleteRemote(a.path, a.remote, a.names, a.expected);
    case 'search_capabilities': return { perl: true };
    case 'search_start': return jobs.start(a.request);
    case 'search_cancel': return jobs.cancel(a.id);
    case 'search_cancel_all': return jobs.cancelAll();
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
  window.__listeners = event => (listeners[event] ?? []).filter(Boolean).length;
});

mkdirSync(shots, { recursive: true });
const shot = async name => { await page.waitForTimeout(400); return page.screenshot({ path: join(shots, `${theme}-${width}-${name}.png`) }); };
const check = (label, ok, detail = '') => { console.log(`${ok ? 'PASS' : 'FAIL'} ${label} ${detail}`); if (!ok) process.exitCode = 1; };
const branches = dir => git(dir, 'for-each-ref', '--format=%(refname:short)', 'refs/heads').split('\n').filter(Boolean);

await page.goto(url);
await page.waitForTimeout(5500);
await page.waitForSelector('.fm-row[aria-label^="Repository"]');
await page.waitForTimeout(800);
await shot('set');

// 1. Cleanup from the row menu on a repository with a remote.
await page.hover('.fm-row[aria-label="Repository alpha"]');
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

// 2. Remote section with its own confirmation.
await page.click('.cleanup-tabs [role=tab]:has-text("Remote")');
await page.waitForSelector('.cleanup-list[aria-label="Branches on origin"]');
const remoteOffered = await page.$$eval('.cleanup-list .cleanup-row .cleanup-name', nodes => nodes.map(node => node.textContent));
check('remote lists merged branches without main', remoteOffered.join() === 'feature/done,old-remote', remoteOffered.join());
check('nothing preselected on the remote', (await page.$$('.cleanup-row input:checked')).length === 0);
await page.check('.cleanup-row:has-text("old-remote") input');
await shot('remote-preview');
await page.click('.cleanup-dialog footer .btn.danger');
await page.waitForSelector('.confirm-dialog[open]');
const confirmText = await page.locator('.confirm-dialog').innerText();
check('confirmation names remote and branch', confirmText.includes('origin in alpha: old-remote'), confirmText.slice(0, 120));
await shot('remote-confirm');
await page.click('.confirm-dialog .btn.danger');
await page.waitForSelector('.cleanup-repo [role=status]:has-text("1 deleted")');
check('remote branch deleted', !execFileSync('git', ['ls-remote', '--heads', remote], { encoding: 'utf8' }).includes('old-remote'));
await page.click('.cleanup-dialog footer .btn:not(.danger):not(.dark)');
await page.waitForSelector('.cleanup-dialog', { state: 'detached' });

// 3. Set-wide cleanup with one expected failure.
await page.check('input[aria-label="Select all shown repositories"]');
await page.click('.fm-bar button[aria-label="Clean up merged branches"]');
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
await page.keyboard.press('Control+Shift+F');
await page.waitForSelector('.code-search');
await page.fill('.cs-pattern input', 'TODO');
await page.click('.cs-bar button[type=submit]');
await page.waitForSelector('.cs-status:has-text("matches in 3 repositories")');
const kinds = await page.$$eval('.cs-list .vrow > div', nodes => nodes.map(node => node.className.replace('cs-row ', '')));
check('grouped by repository then file', kinds.filter(kind => kind.startsWith('cs-repo')).length === 3 && kinds.some(kind => kind.startsWith('cs-file')));
await shot('search-results');
await page.fill('.cs-paths input', 'src/*.ts');
await page.click('.cs-bar button[type=submit]');
await page.waitForSelector('.cs-status:has-text("matches in 3 repositories")');
check('path filter excludes README', !(await page.locator('.cs-list').innerText()).includes('README.md'));
await page.click('.cs-match >> nth=0');
await page.waitForSelector('.notices :text("Copied")');
check('click copies the file path', (await page.evaluate(() => navigator.clipboard.readText())).includes(root));
await shot('search-copied');

await page.fill('.cs-pattern input', 'bigtest');
await page.evaluate(() => {
  window.__gaps = []; let last = performance.now();
  const tick = now => { window.__gaps.push(now - last); last = now; window.__raf = requestAnimationFrame(tick); };
  window.__raf = requestAnimationFrame(tick);
});
await page.click('.cs-bar button[type=submit]');
await page.waitForSelector('.cs-status:has-text("Searching")');
for (let step = 0; step < 6; step++) { await page.evaluate(top => { const box = document.querySelector('.cs-list .vbox'); box.scrollTop = top; }, step * 40000); await page.waitForTimeout(250); }
await shot('search-streaming');
await page.waitForSelector('.cs-status:has-text("10000 matches"), .cs-status:has-text("9999 matches")', { timeout: 30000 });
const gaps = await page.evaluate(() => { cancelAnimationFrame(window.__raf); return window.__gaps.slice(2); });
const worst = Math.max(...gaps), over = gaps.filter(gap => gap > 50).length;
check('10000 matches stream without long frames', over <= 2, `frames=${gaps.length} worst=${worst.toFixed(0)}ms over50=${over}`);
const mounted = await page.$$eval('.cs-list .vrow', nodes => nodes.length);
check('only visible rows are mounted', mounted < 120, `rows=${mounted}`);
await shot('search-big-done');

await page.click('.cs-bar button[type=submit]');
await page.waitForSelector('.cs-bar button.danger');
await page.waitForTimeout(300);
await page.click('.cs-bar button.danger');
await page.waitForSelector('.cs-status:has-text("Cancelled")');
check('stop cancels the job', jobs.log.some(entry => entry.startsWith('cancel')), jobs.log.join(' '));

await page.click('.cs-bar button[type=submit]');
await page.waitForSelector('.cs-bar button.danger');
await page.waitForTimeout(300);
const before = jobs.running.size;
await page.click('.shell-tab:has-text("Code search") .tab-close');
await page.waitForTimeout(600);
check('closing the tab leaves no running job', before === 1 && jobs.running.size === 0, `before=${before} after=${jobs.running.size} ${jobs.log.slice(-3).join(' ')}`);
check('closing the tab removes the listeners', (await page.evaluate(() => window.__listeners('search-matches'))) === 0);

// 5. Four-search limit.
for (let count = 0; count < 4; count++) jobs.start({ pattern: bigPattern(), repos: [{ path: repos.alpha }] });
await page.keyboard.press('Control+Shift+F');
await page.waitForSelector('.code-search');
await page.fill('.cs-pattern input', 'TODO');
await page.click('.cs-bar button[type=submit]');
await page.waitForSelector('.banner.err:has-text("Too many searches")');
await shot('search-limit');
await page.click('button:has-text("Stop all searches and retry")');
await page.waitForSelector('.cs-status:has-text("matches in")', { timeout: 20000 });
check('limit error offers a way out', true);

check('no unmocked commands', unknown.size === 0, [...unknown].join());
await browser.close();
function bigPattern() { return 'bigtest'; }
