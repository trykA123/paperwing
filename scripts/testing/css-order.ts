import { strict as assert } from 'node:assert';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, relative, resolve } from 'node:path';

const root = resolve(import.meta.dir, '../..');
const entry = resolve(root, 'src/app.css');
const manifest = JSON.parse(readFileSync(resolve(root, 'src/styles/order.json'), 'utf8'));
const digest = (text: string) => createHash('sha256').update(text).digest('hex');

export function normalize(text: string, file: string) {
  return text.replace(/\r\n?/g, '\n').replace(/url\(\s*(['"]?)([^)'"\s]+)\1\s*\)/g, (match, _quote, target) => {
    if (/^(?:[a-z]+:|\/|#)/i.test(target)) return match;
    const asset = relative(root, resolve(dirname(file), target)).replaceAll('\\', '/');
    return `url(${JSON.stringify(asset)})`;
  });
}

export function flatten(file: string, seen = new Set<string>()): string {
  const path = resolve(file);
  assert.ok(!seen.has(path), `Duplicate or circular CSS import: ${relative(root, path)}`);
  seen.add(path);
  const source = readFileSync(path, 'utf8').replace(/\r\n?/g, '\n');
  let start = 0, output = '';
  const imports = /^@import\s+['"]([^'"]+)['"];\n?/gm;
  for (const match of source.matchAll(imports)) {
    output += normalize(source.slice(start, match.index), path);
    assert.ok(match[1].startsWith('./'), 'Only local relative chapter imports are supported');
    output += flatten(resolve(dirname(path), match[1]), seen);
    start = match.index! + match[0].length;
  }
  return output + normalize(source.slice(start), path);
}

function verify(source: string) {
  assert.equal(digest(source), manifest.normalizedSourceSHA256, 'Flattened CSS differs from the captured source');
}

function selfTest() {
  const source = flatten(entry);
  verify(source);
  assert.throws(() => verify(source + '\n'));
  assert.throws(() => verify(source.replace('flex: 1;', 'flex: 2;')));
  const split = source.indexOf(':root');
  assert.ok(split > 0);
  assert.throws(() => verify(source.slice(split) + source.slice(0, split)));
  assert.equal(normalize("url('../app-icon.svg')", entry), normalize("url('../../app-icon.svg')", resolve(root, 'src/styles/base.css')));
  assert.throws(() => verify(source.replace('app-icon.svg', 'missing-icon.svg')));
  const seen = new Set([entry]);
  assert.throws(() => flatten(entry, seen));
  let next = 1;
  for (const chapter of manifest.chapters) {
    assert.equal(chapter.firstLine, next);
    assert.ok(chapter.lastLine >= next);
    next = chapter.lastLine + 1;
  }
  assert.equal(next - 1, manifest.originalLineCount);
  console.log(`CSS order self-test passed: ${manifest.chapters.length} contiguous chapters, ${digest(source)}`);
}

function write() {
  const imports = [...readFileSync(entry, 'utf8').matchAll(/^@import '\.\/styles\/([^']+)';$/gm)].map(match => match[1]);
  let next = 1;
  const chapters = imports.map(path => {
    const file = resolve(root, 'src/styles', path);
    const text = normalize(readFileSync(file, 'utf8'), file);
    const lines = text.split('\n').length - (text.endsWith('\n') ? 1 : 0);
    const chapter = { path, firstLine: next, lastLine: next + lines - 1, normalizedSHA256: digest(text) };
    next += lines;
    return chapter;
  });
  const updated = { ...manifest, originalLineCount: next - 1, normalizedSourceSHA256: digest(flatten(entry)), chapters };
  writeFileSync(resolve(root, 'src/styles/order.json'), JSON.stringify(updated, null, 2) + '\n');
  console.log(`Wrote src/styles/order.json: ${chapters.length} chapters`);
}

if (import.meta.main) {
  try {
    if (process.argv[2] === '--write') write();
    else if (process.argv[2] === '--self-test') selfTest();
    else {
      verify(flatten(entry));
      const imports = readFileSync(entry, 'utf8').match(/^@import .+;$/gm) ?? [];
      assert.deepEqual(imports, manifest.chapters.map((chapter: { path: string }) => `@import './styles/${chapter.path}';`));
      for (const chapter of manifest.chapters) {
        const file = resolve(root, 'src/styles', chapter.path);
        assert.equal(digest(normalize(readFileSync(file, 'utf8'), file)), chapter.normalizedSHA256);
      }
      console.log('CSS manifest and flattened cascade match the captured source');
    }
  } catch (error) {
    console.error(error instanceof Error ? error.message : 'CSS order verification failed');
    process.exitCode = 1;
  }
}
