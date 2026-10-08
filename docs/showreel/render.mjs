import { createRequire } from 'node:module';
import { spawn } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';

const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const here = path.dirname(fileURLToPath(import.meta.url));
const FPS = 30;

const open = async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
  await page.goto(pathToFileURL(path.join(here, 'showreel.html')).href);
  await page.waitForFunction('window.READY === true');
  await page.waitForTimeout(500);
  return { browser, page };
};

const [mode, out, ...rest] = process.argv.slice(2);
const { browser, page } = await open();
if (mode === 'stills') {
  for (const t of rest.map(Number)) {
    await page.evaluate((v) => render(v), t);
    await page.screenshot({ path: path.join(out, `f${String(Math.round(t * 100)).padStart(5, '0')}.png`) });
  }
} else {
  const dur = await page.evaluate('DURATION');
  const ff = spawn('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'image2pipe', '-framerate', String(FPS), '-i', '-',
    '-c:v', 'libx264', '-preset', 'slow', '-crf', '16', '-pix_fmt', 'yuv420p', '-movflags', '+faststart', out],
    { stdio: ['pipe', 'inherit', 'inherit'] });
  for (let i = 0; i < Math.floor(dur * FPS); i++) {
    await page.evaluate((v) => render(v), i / FPS);
    const buf = await page.screenshot({ type: 'png' });
    if (!ff.stdin.write(buf)) await new Promise((r) => ff.stdin.once('drain', r));
  }
  ff.stdin.end();
  await new Promise((r) => ff.on('close', r));
}
await browser.close();
