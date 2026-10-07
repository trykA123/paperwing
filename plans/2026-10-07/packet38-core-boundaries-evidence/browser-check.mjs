import { chromium } from '/home/claud/spikes/editor-spike/node_modules/playwright-core/index.mjs';
import { readFile } from 'node:fs/promises';
import { resolve, extname } from 'node:path';
import assert from 'node:assert/strict';
const root = process.cwd();
const evidence = resolve(root, 'plans/2026-10-07/packet38-core-boundaries-evidence');
const browser = await chromium.launch({ executablePath: '/opt/helium-browser-bin/helium', headless: true, env: { ...process.env, XDG_CONFIG_HOME: resolve(root, '.packet38/browser-config'), XDG_CACHE_HOME: resolve(root, '.packet38/browser-cache') }, args: ['--no-sandbox', '--disable-dev-shm-usage'] });
try {
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.route('http://127.0.0.1:41000/**', async route => {
    const path = new URL(route.request().url()).pathname;
    const file = resolve(root, 'dist', path === '/' ? 'index.html' : path.slice(1));
    assert(file.startsWith(resolve(root, 'dist') + '/'));
    const mime = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.woff2': 'font/woff2', '.svg': 'image/svg+xml' }[extname(file)] ?? 'application/octet-stream';
    await route.fulfill({ body: await readFile(file), contentType: mime });
  });
  await page.addInitScript(() => {
    const sources = [
      { id: 'cloud', name: 'Cloud', kind: 'github', host: 'github.com', orgs: ['admin'], urls: [] },
      { id: 'enterprise-one', name: 'Enterprise One', kind: 'ghe', host: 'one.invalid', orgs: ['admin'], urls: [] },
      { id: 'enterprise-two', name: 'Enterprise Two', kind: 'ghe', host: 'two.invalid', orgs: ['admin'], urls: [] },
      { id: 'manual', name: 'Manual', kind: 'manual', host: '', orgs: [], urls: ['git@manual.invalid:admin/repo.git'] },
    ];
    const saved = () => JSON.parse(localStorage.getItem('packet38-settings') ?? JSON.stringify({ sources, workspace: null }));
    const unsupported = { supported: false, reason: 'Browser fixture' };
    const capabilities = Object.fromEntries(['readCompare', 'edit', 'copy', 'recovery', 'trash'].map(key => [key, unsupported]));
    window.packet38Calls = [];
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main', windowLabel: 'main' } },
      transformCallback: () => 1,
      unregisterCallback: () => {},
      invoke: async (command, args = {}) => {
        window.packet38Calls.push({ command, args });
        if (command.startsWith('plugin:event|')) return 1;
        switch (command) {
          case 'load_settings': return saved();
          case 'save_settings': localStorage.setItem('packet38-settings', JSON.stringify(args.settings)); return;
          case 'platform_info': return { platform: 'linux', separator: '/', capabilities, credentials: { backend: 'secret-service', persistent: true, supported: true } };
          case 'probe_root': return { root: args.root, valid: false, identity: null, casePolicy: 'unknown', capabilities };
          case 'path_identities': return [];
          case 'launch_request': return [];
          case 'activity_snapshot': return [];
          case 'source_revision': return 0;
          case 'credential_status': return { sourceId: args.sourceId, revision: 0, state: 'missing', backend: 'secret-service' };
          case 'list_cached_repos': return null;
          case 'list_repos': return { repos: args.source.enabled === false ? [] : [{ id: `${args.source.id}:admin/repo`, source: args.source.id, org: 'admin', name: 'repo', description: '', url: `git@${args.source.host || 'manual.invalid'}:admin/repo.git`, defaultBranch: 'main', pushedAt: '', archived: false }], fetchedAt: 1, errors: [] };
          case 'local_status': return [];
          case 'diagnostics_status': throw new Error('Command diagnostics_status not found');
          default: return null;
        }
      },
    };
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  });
  await page.goto('http://127.0.0.1:41000/');
  await new Promise(resolve => setTimeout(resolve, 5500));
  await page.getByRole('button', { name: /Settings/, exact: false }).first().click();
  const toggle = page.getByRole('button', { name: 'Enable Enterprise One', exact: true });
  await toggle.waitFor();
  assert.equal(await toggle.getAttribute('aria-pressed'), 'true');
  await toggle.focus();
  await page.keyboard.press('Space');
  await page.waitForFunction(() => document.querySelector('[aria-label="Enable Enterprise One"]')?.getAttribute('aria-pressed') === 'false');
  await page.reload();
  await page.getByRole('button', { name: /Settings/, exact: false }).first().click();
  await toggle.waitFor();
  assert.equal(await toggle.getAttribute('aria-pressed'), 'false');
  assert.equal(await page.getByRole('button', { name: 'Enable Enterprise Two', exact: true }).getAttribute('aria-pressed'), 'true');
  await toggle.focus();
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => document.querySelector('[aria-label="Enable Enterprise One"]')?.getAttribute('aria-pressed') === 'true');
  for (const width of [1440, 1100]) {
    await page.setViewportSize({ width, height: 900 });
    for (const theme of ['light', 'dark']) {
      await page.evaluate(theme => document.documentElement.setAttribute('data-theme', theme), theme);
      await page.screenshot({ path: `${evidence}/settings-${width}-${theme}.png`, fullPage: true });
    }
  }
  assert.deepEqual(errors, []);
  console.log('Settings toggle: Space disables, restart restores disabled state, Enter enables, second host stays enabled. Four screenshots saved; no page errors.');
} finally {
  await browser.close();
}
