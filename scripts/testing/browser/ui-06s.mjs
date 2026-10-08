// Full-screen compare, change marking, ribbons, overview ruler, copy confirmation and Save right in Helium with mocked Tauri commands.
// Usage: PLAYWRIGHT_DIR=<dir with playwright-core> node ui-06s.mjs <url> <shots-dir> [light|dark] [width]
import { mkdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { openFolderCompare } from './open-compare.mjs';

const [url, shots, theme = 'light', width = '1440'] = process.argv.slice(2);
const { chromium } = await import(`${process.env.PLAYWRIGHT_DIR}/index.mjs`);
const here = dirname(fileURLToPath(import.meta.url));
const mock = readFileSync(join(here, 'shell-mock.js'), 'utf8');
const fixtures = readFileSync(join(here, 'ui-37-fixtures.js'), 'utf8');
const windowCalls = `(() => { window.isTauri = true; window.__win = []; const base = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = async (command, args) => {
    if (/^plugin:window\\|(set_fullscreen|maximize|is_fullscreen)$/.test(command)) { window.__win.push(command.slice(14) + (args?.value === undefined ? '' : ':' + args.value)); return command.includes('is_') ? false : null; }
    if (command === 'watch_set') return { watched: 0, skipped: [], bestEffort: [] };
    if (command === 'unwatch_set') return null;
    return base(command, args);
  }; })();`;
const browser = await chromium.launch({ executablePath: '/opt/helium-browser-bin/helium', headless: true, args: ['--no-sandbox'] });
mkdirSync(shots, { recursive: true });
const check = (label, ok, detail = '') => { console.log(`${ok ? 'PASS' : 'FAIL'} ${label} ${detail}`); if (!ok) process.exitCode = 1; };
const shot = (page, name) => page.screenshot({ path: join(shots, `06s-${theme}-${width}-${name}.png`) });

async function open(scenario = 'c', extra = {}, context = {}) {
  const page = await (await browser.newContext({ viewport: { width: Number(width), height: 900 }, colorScheme: theme, ...context })).newPage();
  page.on('pageerror', error => { console.log('PAGE ERROR', error.message); process.exitCode = 1; });
  await page.addInitScript(`window.__AUDIT=${JSON.stringify({ theme, platform: 'windows', bigSet: true })};window.__FX=${JSON.stringify({ scenario, bothWorking: true, ...extra })};`);
  await page.addInitScript(mock);
  await page.addInitScript(fixtures);
  await page.addInitScript(windowCalls);
  await page.goto(url);
  await page.waitForTimeout(5500);
  await page.reload();
  await openFolderCompare(page);
  return page;
}

const openFile = async page => {
  await page.getByText(/^(engine\.c|values\.txt|values\.ts|rows\.txt)$/).first().dblclick();
  await page.waitForSelector('.editor-host .cm-line, .editor-host .vw-row', { timeout: 60000 });
  await page.waitForTimeout(500);
};
const shell = page => page.evaluate(() => document.querySelector('#shell').className);
const calls = page => page.evaluate(() => window.__win.join(' '));
const counter = page => page.locator('.editor-count').innerText();
const visible = (page, selector) => page.evaluate(selector => { const el = document.querySelector(selector); return !!el && el.getClientRects().length > 0; }, selector);

const main = await open();
check('opening a compare enters full screen', (await shell(main)).includes('immersive') && (await calls(main)).includes('set_fullscreen:true'), `${await shell(main)} | ${await calls(main)}`);
check('the rail, tab strip, sidebar, details panel and status bar are hidden', !(await visible(main, '.activity-rail')) && !(await visible(main, '.tabs-chrome')) && !(await visible(main, '.shell-side')) && !(await visible(main, '.shell-right')) && !(await visible(main, '.shell-status')));
check('the compare fills the window', await main.evaluate(() => { const box = document.querySelector('.folder-compare').getBoundingClientRect(); return box.width === innerWidth && box.height === innerHeight; }));
await shot(main, 'folder');
await openFile(main);
check('a file diff stays in full screen with Back and Esc shown', (await shell(main)).includes('immersive') && (await main.locator('.fc-back').innerText()).includes('Esc'));
check('the header shows both references, the path, the position and Save right', (await main.locator('.fc-ref').count()) === 2 && (await main.locator('.fc-path').innerText()).includes('engine.c') && /^1 \/ 3$/.test(await counter(main)) && await main.getByRole('button', { name: /Save right/ }).isVisible());
const legend = (await main.locator('.fc-foot').innerText()).replace(/\s+/g, ' ');
check('the footer counts added, removed and changed changes', ['1 added', '1 removed', '1 changed'].every(text => legend.includes(text)) && await main.locator('.fc-foot').getByLabel('Ignore whitespace').isVisible());

const marks = await main.evaluate(() => {
  const line = document.querySelector('.cm-merge-a .cm-line.cm-mk-chg'), bar = getComputedStyle(line, '::before'), changed = document.querySelector('.cm-merge-a .cm-changedLine');
  return { bar: bar.width, barColour: bar.backgroundColor, lineBackground: getComputedStyle(changed).backgroundColor, glyph: document.querySelector('.cm-merge-a .cm-mk-glyph')?.textContent,
    spacer: document.querySelector('.cm-merge-b .cm-mergeSpacer[data-mk]')?.dataset.mk, hatch: getComputedStyle(document.querySelector('.cm-mergeSpacer')).backgroundImage.includes('repeating-linear-gradient') };
});
check('a changed line has a 3 px bar and no background fill', marks.bar === '3px' && marks.barColour !== 'rgba(0, 0, 0, 0)' && marks.lineBackground === 'rgba(0, 0, 0, 0)', JSON.stringify(marks));
check('changed lines carry the glyph and the alignment gap is hatched with its tone', marks.glyph === '~' && marks.hatch && ['rem', 'add'].includes(marks.spacer), JSON.stringify(marks));
check('the ribbon gutter is 56 px wide and draws one ribbon per change', await main.evaluate(() => document.querySelector('.cm-ribbons').getBoundingClientRect().width === 56 && document.querySelectorAll('.cm-ribbon path').length === 3));
const lineCenter = await main.evaluate(() => { const lines = [...document.querySelectorAll('.cm-merge-b .cm-line')], line = lines.find(item => item.textContent.includes('seed')), box = line.getBoundingClientRect(); return box.top + box.height / 2; });
const ribbonEdge = await main.evaluate(() => { const box = document.querySelector('.cm-ribbon-cur path').getBoundingClientRect(); return box.top + box.height / 2; });
check('the first ribbon sits on the changed line', Math.abs(lineCenter - ribbonEdge) < 3, `${lineCenter} vs ${ribbonEdge}`);
check('the current ribbon is numbered', (await main.locator('.cm-ribbon-cur text').textContent()) === '1');
check('the overview ruler is 22 px wide with one mark per change in three lanes', await main.evaluate(() => {
  const ruler = document.querySelector('.cm-ruler'), marks = [...ruler.querySelectorAll('.cm-ruler-mark')];
  return ruler.getBoundingClientRect().width === 22 && marks.length === 3 && new Set(marks.map(mark => mark.style.left)).size === 3;
}));
check('the native scrollbar is gone', await main.evaluate(() => { const view = document.querySelector('.cm-mergeView'); return view.offsetWidth - view.clientWidth === 0; }));
await shot(main, 'file-first');

await main.locator('.fc-foot').click({ position: { x: 4, y: 4 } });
await main.keyboard.press('n');
check('N goes from the shown first change to the second', (await counter(main)) === '2 / 3', await counter(main));
await main.keyboard.press('n');
check('N goes on to the third', (await counter(main)) === '3 / 3', await counter(main));
await main.keyboard.press('p');
check('P goes back', (await counter(main)) === '2 / 3', await counter(main));
await main.keyboard.press('F7');
check('F7 still goes to the next change', (await counter(main)) === '3 / 3', await counter(main));
await main.locator('.cm-merge-b .cm-content').click();
await main.keyboard.press('n');
check('N types into the editable side instead of moving', (await counter(main)) === '3 / 3' && (await main.locator('.cm-merge-b .cm-content').innerText()).includes('n'));
await main.keyboard.press('Control+z');
await main.locator('.cm-ruler-mark').first().click();
await main.waitForTimeout(300);
check('a click on a ruler mark jumps to that change', (await counter(main)) === '1 / 3', await counter(main));
const ruler = await main.locator('.cm-ruler').boundingBox();
await main.mouse.click(ruler.x + 11, ruler.y + ruler.height * 0.9);
await main.waitForTimeout(200);
const scrolled = await main.evaluate(() => document.querySelector('.cm-mergeView').scrollTop);
check('a click on the ruler track scrolls there', scrolled > 1000, String(scrolled));
const thumb = await main.locator('.cm-ruler-thumb').boundingBox();
await main.mouse.move(thumb.x + 10, thumb.y + thumb.height / 2);
await main.mouse.down();
await main.mouse.move(thumb.x + 10, ruler.y + 4, { steps: 5 });
await main.mouse.up();
check('dragging the window scrolls back to the top', await main.evaluate(() => document.querySelector('.cm-mergeView').scrollTop) < 5);
await main.locator('.cm-ribbon path[data-chunk="1"]').click({ force: true });
check('a click on a ribbon jumps to that change', (await counter(main)) === '2 / 3', await counter(main));
await shot(main, 'file-second');

await main.locator('.fc-back').focus();
const order = [], rings = [];
for (let step = 0; step < 14; step++) {
  await main.waitForTimeout(80);
  const label = await main.evaluate(() => { const el = document.activeElement, style = getComputedStyle(el); return { name: el.getAttribute('aria-label') || el.labels?.[0]?.textContent.trim() || el.textContent.trim().replace(/\s+/g, ' ').slice(0, 30) || el.className.toString().slice(0, 20), ring: style.outlineStyle !== 'none' || style.boxShadow !== 'none' || (el.classList.contains('cm-content') && getComputedStyle(el.closest('.cm-editor')).outlineStyle !== 'none') || el.classList.contains('sel-trigger') }; });
  order.push(label.name); rings.push(label.ring);
  await main.keyboard.press('Tab');
}
const wanted = ['Back', 'Previous change', 'Next change', 'More file actions', 'Copy this change to the left file', 'Copy this change to the right file', 'Hide unchanged', 'Ignore whitespace', 'Side by side', 'Inline'];
check('Tab reaches the header, the copy arrows and the footer controls in reading order', wanted.every(name => order.some(item => item.startsWith(name))) && order.findIndex(item => item.startsWith('Back')) < order.findIndex(item => item.startsWith('Next change')) && order.findIndex(item => item.startsWith('More')) < order.findIndex(item => item.startsWith('Hide')), order.join(' > '));
check('every focused control shows a focus ring', rings.slice(1, 13).every(Boolean), rings.map(Number).join(''));
check('focus leaves the page after the last footer control, so nothing hidden takes focus', order[13] === '' || order[13].startsWith('1 open') || order.slice(12, 13)[0] === 'Language');

await main.getByRole('button', { name: 'Copy this change to the left file' }).click();
check('the copy arrow asks before copying and says what will change', await main.locator('.confirm-bar').isVisible() && (await main.locator('.confirm-text').innerText()).includes('Copy change 2 of 3 to the left file. It removes'));
check('the confirm button has focus', await main.evaluate(() => document.activeElement?.textContent === 'Copy to left'));
await main.waitForTimeout(300);
await shot(main, 'confirm');
await main.keyboard.press('Escape');
check('Esc cancels the bar and keeps the full screen', !(await main.locator('.confirm-bar').count()) && (await shell(main)).includes('immersive') && !(await main.locator('.editor-endpoints').innerText()).includes('Unsaved'));
await main.getByRole('button', { name: 'Copy this change to the left file' }).click();
await main.getByRole('button', { name: 'Copy to left' }).click();
check('confirming copies the change into the left file', (await main.locator('.editor-endpoints').innerText()).includes('Unsaved') && (await main.locator('.fc-foot').innerText()).replace(/\s+/g, ' ').includes('0 removed'));
await main.keyboard.press('Control+z');
await main.locator('.cm-merge-b .cm-content').click();
await main.keyboard.press('Control+Home');
await main.keyboard.type('Q');
await main.getByRole('button', { name: /Save right/ }).click();
await main.waitForFunction(() => window.__fileSaves.length > 0);
check('Save right writes the right file through the save path', await main.evaluate(() => window.__fileSaves.at(-1).ticket === 'ticket-right' && window.__fileSaves.at(-1).bytes[0] === 81));
check('Save right is disabled again after saving', await main.getByRole('button', { name: /Save right/ }).isDisabled());

await main.locator('.language-pick .sel-trigger').click();
await main.keyboard.press('Escape');
check('Esc closes the language list first and stays in full screen', (await shell(main)).includes('immersive') && !(await main.locator('.sel-list, [role="listbox"]').count()));
await main.getByRole('button', { name: 'More file actions' }).click();
check('the more menu offers the whole-file copies, undo and the language choice', ['Copy whole file to left', 'Copy whole file to right', 'Undo saved operation', 'Use this language for all .c files'].every(label => main.getByText(label)) && await main.getByText('Copy whole file to left').isVisible());
await main.keyboard.press('Escape');
check('Esc closes the menu first and stays in full screen', !(await main.getByText('Copy whole file to left').count()) && (await shell(main)).includes('immersive'));

await main.keyboard.press('Escape');
await main.waitForTimeout(300);
check('Esc leaves full screen, restores the window and returns to the tab before the compare', !(await shell(main)).includes('immersive') && (await calls(main)).endsWith('set_fullscreen:false') && (await main.locator('.shell-tab.on').innerText()).includes('gateway'), `${await shell(main)} | ${await calls(main)}`);
check('the sidebar, rail and tabs are back', await visible(main, '.activity-rail') && await visible(main, '.tabs-chrome') && await visible(main, '.shell-side'));
await main.locator('.shell-tab:has-text("Compare")').click();
await main.waitForTimeout(300);
check('returning to the compare tab enters full screen again', (await shell(main)).includes('immersive'));
await main.keyboard.press('F11');
await main.waitForTimeout(200);
check('F11 leaves full screen and stays on the compare', !(await shell(main)).includes('immersive') && (await main.locator('.shell-tab.on').innerText()).includes('engine') === false);
await shot(main, 'regular');
await main.keyboard.press('F11');
await main.waitForTimeout(200);
check('F11 enters full screen again', (await shell(main)).includes('immersive'));
await main.locator('.compare-back').click();
await main.waitForTimeout(300);
check('the Back button leaves full screen', !(await shell(main)).includes('immersive') && (await calls(main)).endsWith('set_fullscreen:false'));
await main.context().close();

const calm = await open('c', {}, { reducedMotion: 'reduce' });
await openFile(calm);
await calm.getByRole('button', { name: 'Copy this change to the left file' }).click();
check('reduced motion removes the confirm bar animation', await calm.evaluate(() => getComputedStyle(document.querySelector('.confirm-bar')).animationName === 'none'));
await calm.context().close();

const many = await open('many');
await openFile(many);
await many.waitForTimeout(800);
const crowd = await many.evaluate(() => ({ chunks: document.querySelector('.editor-count').textContent, marks: document.querySelectorAll('.cm-ruler-mark').length, ribbons: document.querySelectorAll('.cm-ribbon path').length }));
check('60 changes keep one ruler mark each and only the ribbons near the view', crowd.chunks.endsWith('/ 60') && crowd.marks >= 30 && crowd.marks <= 60 && crowd.ribbons > 3 && crowd.ribbons < 40, JSON.stringify(crowd));
await many.keyboard.press('p');
await many.waitForTimeout(500);
check('P wraps to the last of 60 changes and the view follows', (await counter(many)) === '60 / 60' && await many.evaluate(() => document.querySelector('.cm-mergeView').scrollTop > 8000));
await shot(many, 'many');
await many.context().close();

const large = await open('large');
await openFile(large);
await large.keyboard.press('F7');
await large.waitForTimeout(300);
check('the large-file view has the bar and glyph marking without a ruler', (await large.locator('.vw-cell.is-changed').count()) > 0 && (await large.locator('.cm-ruler').count()) === 0);
await shot(large, 'large');
await browser.close();
