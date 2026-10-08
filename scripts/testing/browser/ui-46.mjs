// Syntax colouring per file type, the toolbar language picker and the Settings Editor section in Helium with mocked Tauri commands.
// Usage: PLAYWRIGHT_DIR=<dir with playwright-core> node ui-46.mjs <url> <shots-dir> [light|dark] [width]
import { mkdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { openFolderCompare } from './open-compare.mjs';

const [url, shots, theme = 'light', width = '1440'] = process.argv.slice(2);
const { chromium } = await import(`${process.env.PLAYWRIGHT_DIR}/index.mjs`);
const here = dirname(fileURLToPath(import.meta.url));
const mock = readFileSync(join(here, 'shell-mock.js'), 'utf8');
const fixtures = readFileSync(join(here, 'ui-46-fixtures.js'), 'utf8');
const browser = await chromium.launch({ executablePath: '/opt/helium-browser-bin/helium', headless: true, args: ['--no-sandbox'] });
mkdirSync(shots, { recursive: true });
const check = (label, ok, detail = '') => { console.log(`${ok ? 'PASS' : 'FAIL'} ${label} ${detail}`); if (!ok) process.exitCode = 1; };
const shot = (page, name) => page.screenshot({ path: join(shots, `46-${theme}-${width}-${name}.png`) });
const FILES = ['ecu.arxml', 'config.m4', 'powertrain.a2l', 'body.dbc', 'node.can', 'memory.ld', 'app.s19', 'plant.m', 'engine.c'];

async function open(file, extra = {}) {
  const page = await (await browser.newContext({ viewport: { width: 1440, height: 900 }, colorScheme: theme })).newPage();
  page.on('pageerror', error => { console.log('PAGE ERROR', error.message); process.exitCode = 1; });
  await page.addInitScript(`window.__AUDIT=${JSON.stringify({ theme, platform: 'windows', bigSet: true })};window.__FX=${JSON.stringify({ file, ...extra })};`);
  await page.addInitScript(mock);
  await page.addInitScript(fixtures);
  await page.goto(url);
  await page.waitForTimeout(5500);
  await page.reload();
  await openFolderCompare(page);
  await page.getByText(file, { exact: true }).first().dblclick();
  await page.waitForSelector('.editor-host .cm-line', { timeout: 60000 });
  await page.setViewportSize({ width: Number(width), height: 900 });
  for (const name of ['Toggle sidebar', 'Toggle details']) {
    const toggle = page.getByRole('button', { name });
    if (Number(width) < 700 && await toggle.getAttribute('aria-pressed') === 'true') await toggle.click();
  }
  await page.waitForTimeout(800);
  return page;
}

const coloured = page => page.evaluate(() => {
  const spans = [...document.querySelectorAll('.cm-merge-b .cm-line span')];
  const colours = new Set(spans.map(span => getComputedStyle(span).color));
  return { spans: spans.length, colours: colours.size };
});
const pick = page => page.locator('.language-pick .sel-trigger');
const choose = async (trigger, text) => { await trigger.click(); await trigger.page().keyboard.type(text); await trigger.page().keyboard.press('Enter'); };

if (width === '1440') {
  for (const file of FILES) {
    const page = await open(file);
    const { spans, colours } = await coloured(page);
    check(`${file} is coloured`, spans > 3 && colours >= 3, `spans=${spans} colours=${colours}`);
    check(`${file} picker shows the language`, (await pick(page).innerText()).length > 1, await pick(page).innerText());
    await shot(page, file.replace('.', '-'));
    await page.context().close();
  }
  const sniff = await open('notes.zzz');
  check('unknown extension is sniffed as XML', (await pick(sniff).innerText()) === 'XML');
  await choose(pick(sniff), 'A2L');
  await sniff.waitForTimeout(300);
  check('picking a language applies to this file', (await pick(sniff).innerText()) === 'A2L (ASAP2)');
  const before = (await coloured(sniff)).colours;
  await sniff.getByRole('button', { name: 'More file actions' }).click();
  await sniff.locator('label:has-text("Use this language for all .zzz files") input').check();
  await sniff.waitForFunction(() => window.__settings.at(-1)?.settings.workspace.languageMap?.zzz === 'asap2', null, { timeout: 8000 }).catch(() => {});
  const saved = await sniff.evaluate(() => window.__settings.at(-1).settings.workspace.languageMap);
  check('"Use for all" persists the mapping in the workspace', saved?.zzz === 'asap2', JSON.stringify(saved));
  check('the colouring stays after remembering', (await coloured(sniff)).colours >= Math.min(before, 2));
  await shot(sniff, 'picker-remembered');
  await sniff.locator('label:has-text("Use this language for all .zzz files") input').uncheck();
  await sniff.waitForFunction(() => window.__settings.at(-1).settings.workspace.languageMap.zzz === undefined, null, { timeout: 5000 });
  check('unchecking removes the mapping', (await pick(sniff).innerText()) === 'XML');
  await sniff.context().close();
}

if (width === '390') {
  const page = await open('ecu.arxml');
  check('picker is visible on a narrow window', await pick(page).isVisible());
  const overflow = await page.evaluate(() => { return ['.fc-head', '.fc-foot'].some(selector => { const bar = document.querySelector(selector); return bar.scrollWidth > bar.clientWidth + 1; }); });
  check('the header and footer fit without horizontal overflow', !overflow);
  await shot(page, 'toolbar');
  await page.context().close();
}

const settings = await open('engine.c');
await settings.evaluate(() => { const gear = [...document.querySelectorAll('button')].find(button => /settings/i.test(button.getAttribute('aria-label') ?? button.title ?? '')); gear?.click(); });
await settings.waitForTimeout(400);
await settings.getByRole('button', { name: 'Editor' }).click();
await settings.waitForSelector('text=No custom mappings');
await settings.getByLabel('File extension').fill('.CFG');
await choose(settings.getByRole('combobox', { name: 'Language for the new mapping' }), 'XML');
await settings.getByRole('button', { name: 'Add mapping' }).click();
await settings.waitForSelector('text=.cfg');
await settings.getByLabel('File extension').fill('x y');
check('an invalid extension disables Add and explains', (await settings.getByRole('button', { name: 'Add mapping' }).isDisabled()) && await settings.getByText('Use letters, digits').isVisible());
await settings.getByLabel('File extension').fill('');
await settings.waitForFunction(() => window.__settings.at(-1)?.settings.workspace.languageMap?.cfg === 'xml', null, { timeout: 8000 }).catch(() => {});
check('the mapping reaches the saved settings', await settings.evaluate(() => window.__settings.at(-1).settings.workspace.languageMap.cfg === 'xml'));
await shot(settings, 'settings-editor');
await settings.getByRole('button', { name: 'Remove .cfg mapping' }).click();
await settings.waitForSelector('text=No custom mappings');
check('removing a mapping empties the list', true);
await settings.context().close();
await browser.close();
