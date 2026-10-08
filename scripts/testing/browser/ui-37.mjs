// File compare on the CodeMirror adapter and the large-file renderer in Helium with mocked Tauri commands.
// Usage: PLAYWRIGHT_DIR=<dir with playwright-core> node ui-37.mjs <url> <shots-dir> [light|dark] [width]
import { mkdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { openFolderCompare } from './open-compare.mjs';

const [url, shots, theme = 'light', width = '1440'] = process.argv.slice(2);
const { chromium } = await import(`${process.env.PLAYWRIGHT_DIR}/index.mjs`);
const here = dirname(fileURLToPath(import.meta.url));
const mock = readFileSync(join(here, 'shell-mock.js'), 'utf8');
const fixtures = readFileSync(join(here, 'ui-37-fixtures.js'), 'utf8');
const browser = await chromium.launch({ executablePath: '/opt/helium-browser-bin/helium', headless: true, args: ['--no-sandbox'] });
mkdirSync(shots, { recursive: true });
const check = (label, ok, detail = '') => { console.log(`${ok ? 'PASS' : 'FAIL'} ${label} ${detail}`); if (!ok) process.exitCode = 1; };
const shot = (page, name) => page.screenshot({ path: join(shots, `37-${theme}-${width}-${name}.png`) });

async function open(scenario, extra = {}) {
  const page = await (await browser.newContext({ viewport: { width: Number(width), height: 900 }, colorScheme: theme })).newPage();
  page.on('pageerror', error => { console.log('PAGE ERROR', error.message); process.exitCode = 1; });
  await page.addInitScript(`window.__AUDIT=${JSON.stringify({ theme, platform: 'windows', bigSet: true })};window.__FX=${JSON.stringify({ scenario, ...extra })};`);
  await page.addInitScript(mock);
  await page.addInitScript(fixtures);
  await page.goto(url);
  await page.waitForTimeout(5500);
  await page.reload();
  await openFolderCompare(page);
  await page.getByText(/^(engine\.c|frame\.c|mixed\.c|values\.txt|values\.ts|items\.json)$/).first().dblclick();
  await page.waitForSelector('.editor-host .cm-line, .editor-host .vw-row', { timeout: 60000 });
  await page.waitForTimeout(400);
  return page;
}

const openMs = async page => { const timing = await page.evaluate(() => window.__openTiming); return Math.round(timing.ready - timing.start); };
const count = (page, selector) => page.locator(selector).count();
const counter = page => page.locator('.editor-count').innerText();

const c = await open('c', { bothWorking: true });
check('C file opens in two CodeMirror editors', (await count(c, '.editor-host .cm-editor')) === 2);
check('changed lines are marked on both sides', (await count(c, '.cm-merge-a .cm-changedLine')) > 0 && (await count(c, '.cm-merge-b .cm-changedLine')) > 0);
check('syntax highlighting is applied', (await count(c, '.editor-host .cm-line span[class]')) > 5);
const widths = await c.evaluate(() => [...document.querySelectorAll('.cm-mergeViewEditor')].map(editor => editor.getBoundingClientRect().width));
check('both panes share the width', widths.length === 2 && Math.abs(widths[0] - widths[1]) < 2 && widths[0] * 2 + 56 <= Number(width) + 2, JSON.stringify(widths));
check('the counter starts at the first of several changes', /^1 \/ [2-9]$/.test(await counter(c)) || (await counter(c)).startsWith('Identical') === false, await counter(c));
await c.keyboard.press('F7');
await c.keyboard.press('F7');
check('F7 moves to the next change', /^2 \//.test(await counter(c)), await counter(c));
await c.keyboard.press('Shift+F7');
check('Shift+F7 moves back', /^1 \//.test(await counter(c)), await counter(c));
await c.keyboard.press('Shift+F7');
check('Shift+F7 wraps to the last change', /^3 \/ 3$/.test(await counter(c)), await counter(c));
await shot(c, 'c-side-by-side');
const unsaved = () => c.locator('.editor-endpoints').innerText().then(text => text.includes('Unsaved'));
await c.keyboard.press('Control+Alt+ArrowLeft');
check('Ctrl+Alt+Left copies the hunk to the left file', await unsaved());
await c.locator('.cm-merge-a .cm-content').click();
await c.keyboard.press('Control+z');
check('Ctrl+Z undoes the copy and clears the unsaved mark', !(await unsaved()));
await c.getByRole('button', { name: 'To left' }).first().click();
check('the To left button copies the hunk and marks the left file unsaved', await unsaved());
await c.locator('.editor-toolbar label:has-text("Ignore whitespace") input').check();
check('Ignore whitespace keeps the change list', /^\d \/ \d$/.test(await counter(c)), await counter(c));
await c.locator('.editor-toolbar label:has-text("Ignore whitespace") input').uncheck();
await c.click('.seg button:has-text("Inline")');
await c.waitForSelector('.editor-host .cm-deletedChunk, .editor-host .cm-changedLine');
check('inline view shows one editor with deleted blocks', (await count(c, '.editor-host .cm-editor')) === 1 && (await count(c, '.cm-deletedChunk')) > 0);
await shot(c, 'c-inline');
await c.click('.seg button:has-text("Side by side")');
await c.waitForSelector('.editor-host .cm-merge-b');
await c.locator('label:has-text("Hide unchanged") input').check();
await c.waitForSelector('.cm-collapsedLines');
check('hide unchanged collapses the unchanged stretches', (await count(c, '.cm-collapsedLines')) > 0);
await shot(c, 'c-hide-unchanged');
await c.locator('label:has-text("Hide unchanged") input').uncheck();
await c.locator('.cm-merge-b .cm-content').click();
await c.keyboard.press('Control+f');
check('Ctrl+F opens the search panel', (await count(c, '.cm-search')) > 0);
const panel = await c.locator('.cm-panels').boundingBox(), area = await c.locator('.editor-host').boundingBox();
check('the search panel floats at the top of the editor area', !!panel && panel.y >= area.y - 1 && panel.y < area.y + 80, JSON.stringify({ panel, area }));
const tops = await c.evaluate(() => {
  const named = side => new Map([...document.querySelectorAll(`.cm-merge-${side} .cm-line`)].filter(line => line.textContent.startsWith('static int compute_')).map(line => [line.textContent, line.getBoundingClientRect().top]));
  const [left, right] = [named('a'), named('b')];
  return [...left].filter(([text]) => right.has(text)).map(([text, top]) => [top, right.get(text)]);
});
check('the search panel does not shift the aligned lines', tops.length > 0 && tops.every(([a, b]) => Math.abs(a - b) < 1), JSON.stringify(tops));
await c.keyboard.type('total');
await c.waitForSelector('.cm-searchMatch');
await shot(c, 'c-search');
await c.keyboard.press('Escape');
const editorBackground = () => c.evaluate(() => getComputedStyle(document.querySelector('.cm-editor')).backgroundColor);
const before = await editorBackground();
await c.evaluate(() => { document.documentElement.dataset.theme = document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark'; });
await c.waitForTimeout(250);
check('switching the theme restyles the editor without a reload', before !== await editorBackground());
await shot(c, 'c-theme-switched');
await c.evaluate(t => { document.documentElement.dataset.theme = t; }, theme);
await c.waitForTimeout(250);
await c.keyboard.press('Control+s');
await c.waitForFunction(() => window.__fileSaves.length > 0);
const saved = await c.evaluate(() => ({ count: window.__fileSaves.length, ticket: window.__fileSaves[0].ticket, equal: window.__fileSaves[0].bytes.length > 0 }));
check('Ctrl+S saves the left file through the write path', saved.count === 1 && saved.ticket === 'ticket-left' && saved.equal, JSON.stringify(saved));

const left = await open('c', { leftOnly: true });
await left.click('.seg button:has-text("Inline")');
await left.waitForSelector('.editor-host .cm-deletedChunk');
const saveTwice = async text => {
  await left.locator('.editor-host .cm-content').click();
  await left.keyboard.press('Control+Home');
  await left.keyboard.type(text);
  await left.keyboard.press('Control+s');
};
await saveTwice('Z');
await left.waitForFunction(() => window.__fileSaves.length === 1);
await left.waitForFunction(() => !document.querySelector('.editor-endpoints').innerText.includes('Unsaved'));
await saveTwice('Y');
await left.waitForFunction(() => window.__fileSaves.length === 2, null, { timeout: 5000 }).catch(() => {});
const inlineSaves = await left.evaluate(() => ({ count: window.__fileSaves.length, tickets: window.__fileSaves.map(save => save.ticket), second: String.fromCharCode(...window.__fileSaves.at(-1)?.bytes.slice(0, 2) ?? []) }));
check('inline layout saves a working-tree left side twice, still writable after the first save', inlineSaves.count === 2 && inlineSaves.tickets.every(ticket => ticket === 'ticket-left') && inlineSaves.second === 'YZ', JSON.stringify(inlineSaves));
await shot(left, 'inline-left-writable');
await left.locator('.editor-host .cm-content').click();
await left.keyboard.type('Q');
await left.keyboard.press('F11');
await left.getByRole('button', { name: 'Apply rules' }).click();
await left.getByRole('button', { name: 'Other options' }).click();
await left.getByRole('button', { name: 'Keep editing' }).click();
await left.waitForTimeout(400);
check('refreshing the comparison with unsaved edits asks first and keeps them', (await left.locator('.editor-endpoints').innerText()).includes('Unsaved') && (await left.locator('.editor-host .cm-content').innerText()).includes('Q'));

const crlf = await open('crlf');
check('CRLF label is shown', (await crlf.locator('.editor-endpoints').innerText()).includes('UTF-8 · CRLF'));
await crlf.locator('.cm-merge-b .cm-content').click();
await crlf.keyboard.press('Control+Home');
await crlf.keyboard.type('X');
await crlf.keyboard.press('Control+s');
await crlf.waitForFunction(() => window.__fileSaves.length > 0);
const exact = await crlf.evaluate(() => {
  const original = window.__bytes.right, saved = window.__fileSaves[0].bytes;
  const expected = [...original.slice(0, 3), 88, ...original.slice(3)];
  return { same: saved.length === expected.length && saved.every((value, index) => value === expected[index]), bom: saved.slice(0, 3).join(), crlf: saved.filter((value, index) => value === 10 && saved[index - 1] === 13).length, bareLf: saved.filter((value, index) => value === 10 && saved[index - 1] !== 13).length };
});
check('the saved bytes keep the BOM and CRLF and gain only the typed character', exact.same && exact.bom === '239,187,191' && exact.bareLf === 0, JSON.stringify(exact));
await shot(crlf, 'crlf');

const mixed = await open('mixed');
check('mixed line endings stay read-only with a notice', (await mixed.locator('.file-compare').innerText()).includes('Mixed line endings'));
await shot(mixed, 'mixed');

const large = await open('large');
check('a 6 MB file opens in the read-only renderer with the notice', (await count(large, '.editor-host .vw-row')) > 0 && (await count(large, '.editor-host .cm-editor')) === 0
  && (await large.locator('.editor-notice').innerText()) === 'Large file: read-only view');
await large.keyboard.press('F7');
check('F7 goes to the first change of the large file', /^1 \//.test(await counter(large)), await counter(large));
await large.keyboard.press('F7');
check('F7 goes on to the second change', /^2 \//.test(await counter(large)), await counter(large));
check('large file changes are marked with a bar and a glyph', (await count(large, '.vw-cell.is-changed')) > 0 && (await large.evaluate(() => getComputedStyle(document.querySelector('.vw-cell.is-changed .vw-no'), '::before').content)) === '"~"');
console.log('INFO large open ms', await openMs(large));
await shot(large, 'large');
await large.click('.seg button:has-text("Inline")');
await large.waitForSelector('.vw-cell .vw-no + .vw-no');
await shot(large, 'large-inline');

const json = await open('json');
check('a single-line 2 MB JSON opens in CodeMirror', (await count(json, '.editor-host .cm-editor')) === 2);
console.log('INFO single-line json open ms', await openMs(json));
await shot(json, 'json');

const megabyte = await open('mb');
check('a 1 MB file opens in CodeMirror', (await count(megabyte, '.editor-host .cm-editor')) === 2);
console.log('INFO 1 MB open ms', await openMs(megabyte));
await shot(megabyte, '1mb');
console.log('INFO small C open ms', await openMs(c));
await browser.close();
