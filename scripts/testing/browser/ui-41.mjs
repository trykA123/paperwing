// Settings > Diagnostics in Helium with mocked Tauri commands.
// Usage: PLAYWRIGHT_DIR=<dir with playwright-core> node ui-41.mjs <url> <shots-dir> <light|dark> <width> [absent]
import { strict as assert } from 'node:assert';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';

const [url, shots, theme = 'light', width = '1440', mode = 'present'] = process.argv.slice(2);
const { chromium } = await import(`${process.env.PLAYWRIGHT_DIR}/index.mjs`);
const caps = Object.fromEntries(['readCompare', 'edit', 'copy', 'recovery', 'trash'].map(name => [name, { supported: true, reason: null }]));
const document = JSON.stringify({ version: 1, machine: { os: 'windows', build: 26100, cores: 16, ramGb: 32, drive: 'ssd', git: '2.47.1', skein: '0.2.0' }, scale: { sets: 2, repos: Array.from({ length: 8 }, (_, i) => ({ repo: `repo-${i + 1}`, trackedFileCount: 1500 * (i + 1), refCount: 43, looseObjectBytes: 120000, packedObjectBytes: 43000000, worktree: i % 3 === 0 })) } }, null, 2);
let refuse = false, cancelled = false;
const states = { status: mode === 'present' };
const log = [];

const browser = await chromium.launch({ executablePath: '/opt/helium-browser-bin/helium', headless: true, args: ['--no-sandbox'] });
const page = await (await browser.newContext({ viewport: { width: Number(width), height: 900 }, colorScheme: theme })).newPage();
const noise = [];
page.on('pageerror', error => noise.push(`PAGE ERROR ${error.message}`));
page.on('console', message => { if (['error', 'warning', 'debug'].includes(message.type())) noise.push(`${message.type()} ${message.text()}`); });
await page.exposeFunction('__backend', async (cmd, a) => {
  log.push(cmd);
  switch (cmd) {
    case 'load_settings': return { sources: [], workspace: { sets: [], activeSet: '', root: '/tmp/x', theme, pageSize: 'all' } };
    case 'platform_info': return { platform: 'windows', separator: '\\', capabilities: caps, credentials: { backend: 'unsupported', persistent: false, supported: false, reason: null } };
    case 'diagnostics_status': if (!states.status) throw `Command ${cmd} not found`; return { sampling: true, samples: 1834 };
    case 'diagnostics_preview': {
      cancelled = false;
      for (let step = 1; step <= 4; step += 1) { await new Promise(r => setTimeout(r, 700));
        if (cancelled) throw 'Diagnostics collection cancelled'; await page.evaluate(([n]) => window.__emit('diagnostics-progress', { phase: 'scale', completed: n, total: 24 }), [step * 6]).catch(() => {}); }
      if (refuse) throw 'Diagnostics contained identifying text; nothing was written';
      return document;
    }
    case 'diagnostics_cancel': cancelled = true; return true;
    case 'diagnostics_export': return true;
    case 'credential_status': return { sourceId: a.sourceId, backend: 'unsupported', state: 'unavailable', revision: 0, reason: null };
    case 'path_identities': return (a.paths ?? []).map(path => ({ path, identity: path, exists: true, reason: null }));
    case 'probe_root': return { root: a.root, valid: true, reason: null, identity: 'x', casePolicy: 'sensitive', capabilities: caps };
    case 'search_capabilities': return { perl: true };
    case 'source_revision': return 0;
    case 'activity_snapshot': case 'get_refs_many': case 'launch_request': return [];
    default: return null;
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
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__emit = (event, payload) => (listeners[event] ?? []).forEach((handler, index) => handler && window['_' + handler]({ event, id: index + 1, payload }));
});
mkdirSync(shots, { recursive: true });
const shot = async name => { await page.waitForTimeout(400); await page.screenshot({ path: join(shots, `${theme}-${width}-${mode}-${name}.png`) }); };
const check = (label, ok, detail = '') => { console.log(`${ok ? 'PASS' : 'FAIL'} ${label} ${detail}`); if (!ok) process.exitCode = 1; };

await page.goto(url);
await page.waitForTimeout(5500);
await page.click('button[aria-label="Settings"]');
await page.waitForSelector('.settings-nav');
await page.waitForTimeout(500);
const nav = await page.locator('.settings-nav').innerText();
if (mode === 'absent') {
  check('no Diagnostics entry without the command', !nav.includes('Diagnostics'), nav.replace(/\n/g, ','));
  check('no error toast', (await page.locator('.toast, [role=status].err').count()) === 0);
  check('console quiet beyond one debug line', noise.filter(line => !line.includes('[vite]') && !line.startsWith('debug Diagnostics are not part')).length === 0 && noise.filter(line => line.startsWith('debug Diagnostics')).length === 1, noise.join(' | '));
  await shot('nav');
  await browser.close();
  process.exit();
}
check('Diagnostics entry present', nav.includes('Diagnostics'));
await page.click('.settings-nav button:has-text("Diagnostics")');
await page.waitForSelector('.diag-samples');
check('sample count shown', (await page.locator('.diag-samples').innerText()).includes('1,834'));
await shot('idle');
await page.click('button:has-text("Generate diagnostics")');
await page.waitForSelector('.diag-progress .bar i');
await page.waitForTimeout(1600);
await shot('progress');
await page.waitForSelector('.diag-json');
const bytes = await page.locator('.diag-preview-head .hint').innerText();
check('byte size shown', /\d+(\.\d)? (B|KB)/.test(bytes), bytes);
check('preview is the exact document', (await page.locator('.diag-json').innerText()) === document);
check('block is scrollable', await page.$eval('.diag-json', node => node.scrollHeight > node.clientHeight));
await shot('preview');
await page.click('button:has-text("Save file")');
await page.waitForSelector('text=Diagnostics saved');
await shot('saved');
refuse = true;
await page.click('button:has-text("Generate diagnostics")');
await page.waitForSelector('.banner.err');
check('refusal shown plainly', (await page.locator('.banner.err').innerText()).includes('identifying text'));
check('no preview after refusal', (await page.locator('.diag-json').count()) === 0);
await shot('refused');
refuse = false;
await page.click('button:has-text("Generate diagnostics")');
await page.waitForSelector('button:has-text("Cancel")');
await page.click('button:has-text("Cancel")');
await page.waitForTimeout(3500);
check('cancel called', log.includes('diagnostics_cancel'));
check('cancel leaves no error and no preview', (await page.locator('.banner.err, .diag-json').count()) === 0);
await shot('cancelled');
console.log(noise.join('\n'));
await browser.close();
