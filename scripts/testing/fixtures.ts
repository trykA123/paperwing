import { createHash } from 'node:crypto';
import { strict as assert } from 'node:assert';
import { spawnSync } from 'node:child_process';
import { chmodSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, renameSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, join, resolve, sep } from 'node:path';

export const fixtureMarker = 'paperwing-disposable-fixture-v1\n';
export type Manifest = { version: 1; scale: number; content: string; objects: string; head: string; base: string; orphan: string; restore: { path: 'sacrificial.txt'; backup: 'sacrificial.backup'; sha256: string } };
export const sha256 = (bytes: string | Uint8Array) => createHash('sha256').update(bytes).digest('hex');

export function noSymlinks(path: string) {
  const target = resolve(path);
  let current = target;
  while (true) {
    if (existsSync(current) || (() => { try { lstatSync(current); return true; } catch { return false; } })()) {
      if (lstatSync(current).isSymbolicLink()) throw new Error('Refusing symlink path');
    }
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
  return target;
}

export function markedRoot(root: string, marker = fixtureMarker) {
  const target = noSymlinks(root);
  if (!isAbsolute(root) || !lstatSync(target).isDirectory()) throw new Error('Expected absolute disposable directory');
  const file = noSymlinks(join(target, '.paperwing-disposable'));
  if (readFileSync(file, 'utf8') !== marker) throw new Error('Disposable marker mismatch');
  return realpathSync(target);
}

export function newRoot(root: string, marker = fixtureMarker) {
  const target = noSymlinks(root);
  if (!isAbsolute(root) || existsSync(target)) throw new Error('Refusing existing or relative root');
  mkdirSync(target, { recursive: true });
  writeFileSync(join(target, '.paperwing-disposable'), marker, { flag: 'wx' });
  return target;
}

export function fixtureGit(root: string, cwd: string, args: string[]) {
  const environment = { ...process.env };
  for (const name of Object.keys(environment)) if (name.startsWith('GIT_')) delete environment[name];
  const result = spawnSync('git', ['-c', 'core.attributesFile=/dev/null', '-c', 'core.autocrlf=false', '-c', 'core.eol=lf', '-c', 'core.filemode=true', '-c', 'commit.gpgsign=false', '-c', 'tag.gpgsign=false', '-c', 'gc.auto=0', '-c', 'core.hooksPath=' + join(root, 'empty'), ...args], {
    cwd, encoding: 'utf8', env: { ...environment, GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: '/dev/null', GIT_CONFIG_COUNT: '0', GIT_ATTR_NOSYSTEM: '1', GIT_TEMPLATE_DIR: join(root, 'empty'), GIT_AUTHOR_NAME: 'Fixture', GIT_AUTHOR_EMAIL: 'fixture@example.invalid', GIT_COMMITTER_NAME: 'Fixture', GIT_COMMITTER_EMAIL: 'fixture@example.invalid', GIT_AUTHOR_DATE: '2000-01-01T00:00:00+00:00', GIT_COMMITTER_DATE: '2000-01-01T00:00:00+00:00', GIT_TERMINAL_PROMPT: '0', LC_ALL: 'C', TZ: 'UTC' },
  });
  if (result.error || result.status !== 0) throw new Error('Fixture Git command failed');
  return result.stdout.trim();
}

function contentFingerprint(root: string) {
  const hash = createHash('sha256');
  const walk = (directory: string) => {
    for (const name of readdirSync(directory).sort()) {
      if (name === '.git') continue;
      const path = noSymlinks(join(directory, name)), stat = lstatSync(path);
      if (stat.isDirectory()) walk(path);
      else {
        hash.update(path.slice(root.length + 1).split(sep).join('/'));
        hash.update(stat.mode & 0o111 ? '\0executable\0' : '\0regular\0');
        hash.update(sha256(readFileSync(path)));
      }
    }
  };
  walk(join(root, 'checkouts'));
  return hash.digest('hex');
}

export function fingerprints(root: string) {
  markedRoot(root);
  const inspect = (directory: string) => {
    for (const name of readdirSync(noSymlinks(directory))) {
      const path = noSymlinks(join(directory, name));
      if (lstatSync(path).isDirectory()) inspect(path);
    }
  };
  for (const path of ['source.git', 'seed', 'checkouts']) inspect(join(root, path));
  for (const metadata of ['source.git', 'seed/.git', 'checkouts/left/.git', 'checkouts/right/.git']) {
    if (!lstatSync(join(root, metadata)).isDirectory() || existsSync(join(root, metadata, 'objects/info/alternates'))) throw new Error('Fixture metadata must be independent local directories');
  }
  const objects = fixtureGit(root, join(root, 'source.git'), ['rev-list', '--all', '--objects']).split('\n').map(line => line.split(' ')[0]).sort().join('\n');
  return { content: contentFingerprint(root), objects: sha256(objects) };
}

export function replay(root: string): Manifest {
  markedRoot(root);
  const manifest: Manifest = JSON.parse(readFileSync(noSymlinks(join(root, 'manifest.json')), 'utf8'));
  assert.equal(manifest.version, 1);
  assert.deepEqual(fingerprints(root), { content: manifest.content, objects: manifest.objects });
  assert.equal(manifest.restore.path, 'sacrificial.txt');
  assert.equal(manifest.restore.backup, 'sacrificial.backup');
  assert.equal(fixtureGit(root, join(root, 'checkouts', 'left'), ['rev-parse', 'HEAD']), manifest.base);
  assert.equal(fixtureGit(root, join(root, 'checkouts', 'right'), ['rev-parse', 'HEAD']), manifest.head);
  assert.equal(sha256(readFileSync(noSymlinks(join(root, manifest.restore.path)))), manifest.restore.sha256);
  assert.equal(sha256(readFileSync(noSymlinks(join(root, manifest.restore.backup)))), manifest.restore.sha256);
  return manifest;
}

export function restoreDrill(root: string) {
  const manifest = replay(root);
  const target = noSymlinks(join(root, manifest.restore.path)), backup = noSymlinks(join(root, manifest.restore.backup));
  const original = readFileSync(target), saved = readFileSync(backup);
  assert.equal(sha256(saved), manifest.restore.sha256);
  writeFileSync(target, saved);
  assert.deepEqual(readFileSync(target), original);
  try { writeFileSync(target, 'manifest-owned sacrificial mutation\n'); }
  finally { writeFileSync(target, saved); }
  assert.deepEqual(readFileSync(target), original);
  replay(root);
}

export function createFixtures(root: string, scale = 512): Manifest {
  if (!Number.isInteger(scale) || scale < 8 || scale > 5000) throw new Error('Scale must be 8..5000');
  newRoot(root);
  mkdirSync(join(root, 'empty'));
  mkdirSync(join(root, 'seed'));
  mkdirSync(join(root, 'checkouts'));
  const seed = join(root, 'seed');
  fixtureGit(root, seed, ['init', '--object-format=sha1', '--initial-branch=main']);
  writeFileSync(join(seed, '.gitattributes'), '* -text\n');
  mkdirSync(join(seed, 'normalized'));
  for (let index = 0; index < scale; index++) writeFileSync(join(seed, 'normalized', `${String(index).padStart(5, '0')}.txt`), `fixture ${index}\nalpha beta\n`);
  writeFileSync(join(seed, 'binary.bin'), Buffer.from([0, 1, 2, 128, 255]));
  writeFileSync(join(seed, 'rename-before.txt'), 'rename content\n');
  writeFileSync(join(seed, 'mode.sh'), 'echo fixture\n');
  writeFileSync(join(seed, 'left-only.txt'), 'left only\n');
  fixtureGit(root, seed, ['add', '.']);
  fixtureGit(root, seed, ['commit', '-m', 'base']);
  const base = fixtureGit(root, seed, ['rev-parse', 'HEAD']);
  renameSync(join(seed, 'rename-before.txt'), join(seed, 'rename-after.txt'));
  chmodSync(join(seed, 'mode.sh'), 0o755);
  writeFileSync(join(seed, 'normalized', '00000.txt'), 'fixture 0\r\nalpha beta\r\n');
  writeFileSync(join(seed, 'normalized', '00001.txt'), 'fixture 1\nalpha   beta\n');
  writeFileSync(join(seed, 'binary.bin'), Buffer.from([0, 1, 3, 128, 255]));
  writeFileSync(join(seed, 'right-only.txt'), 'right only\n');
  fixtureGit(root, seed, ['rm', 'left-only.txt']);
  fixtureGit(root, seed, ['add', '.']);
  fixtureGit(root, seed, ['commit', '-m', 'changes']);
  const head = fixtureGit(root, seed, ['rev-parse', 'HEAD']);
  const tree = fixtureGit(root, seed, ['rev-parse', 'HEAD^{tree}']);
  const orphan = fixtureGit(root, seed, ['commit-tree', tree, '-m', 'unrelated history']);
  fixtureGit(root, seed, ['update-ref', 'refs/heads/orphan', orphan]);
  fixtureGit(root, root, ['clone', '--bare', '--no-hardlinks', '--', seed, join(root, 'source.git')]);
  for (const name of ['left', 'right']) fixtureGit(root, root, ['clone', '--no-hardlinks', '--', join(root, 'source.git'), join(root, 'checkouts', name)]);
  fixtureGit(root, join(root, 'checkouts', 'left'), ['checkout', '--detach', base]);
  const sacrificial = Buffer.from('sacrificial restore fixture\n');
  writeFileSync(join(root, 'sacrificial.txt'), sacrificial);
  writeFileSync(join(root, 'sacrificial.backup'), sacrificial);
  const manifest: Manifest = { version: 1, scale, ...fingerprints(root), base, head, orphan, restore: { path: 'sacrificial.txt', backup: 'sacrificial.backup', sha256: sha256(sacrificial) } };
  writeFileSync(join(root, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n', { flag: 'wx' });
  restoreDrill(root);
  return manifest;
}

function selfTest() {
  const parent = mkdtempSync(join(tmpdir(), 'paperwing-fixtures-self-test-'));
  const one = createFixtures(join(parent, 'one'), 16), two = createFixtures(join(parent, 'two'), 16);
  assert.deepEqual(one, two);
  const originalXdg = process.env.XDG_CONFIG_HOME;
  const hostile = join(parent, 'hostile-xdg');
  mkdirSync(join(hostile, 'git'), { recursive: true });
  writeFileSync(join(hostile, 'git', 'attributes'), '* working-tree-encoding=UTF-16\n');
  try {
    process.env.XDG_CONFIG_HOME = hostile;
    assert.deepEqual(createFixtures(join(parent, 'hostile-fixture'), 16), one);
  } finally {
    if (originalXdg === undefined) delete process.env.XDG_CONFIG_HOME;
    else process.env.XDG_CONFIG_HOME = originalXdg;
  }
  assert.equal(process.env.XDG_CONFIG_HOME, originalXdg);
  assert.throws(() => createFixtures(join(parent, 'one'), 16));
  const unmarked = join(parent, 'unmarked'); mkdirSync(unmarked);
  assert.throws(() => replay(unmarked));
  const link = join(parent, 'link'); symlinkSync(join(parent, 'one'), link);
  assert.throws(() => replay(link));
  assert.throws(() => createFixtures(join(parent, 'bad-scale'), NaN));
  restoreDrill(join(parent, 'one'));
  writeFileSync(join(parent, 'one', 'checkouts', 'right', 'right-only.txt'), 'changed');
  assert.throws(() => replay(join(parent, 'one')));
  console.log('fixtures self-test passed, including hostile user attributes and environment restoration; disposable artifacts retained');
}

if (import.meta.main) {
  try {
    const [command, root, scale] = process.argv.slice(2);
    if (command === '--self-test') selfTest();
    else if (command === '--create' && root) console.log(JSON.stringify(createFixtures(root, scale === undefined ? 512 : Number(scale))));
    else if (command === '--replay' && root) console.log(JSON.stringify(replay(root)));
    else if (command === '--restore-drill' && root) { restoreDrill(root); console.log('sacrificial restore verified'); }
    else throw new Error('Usage: fixtures.ts --create ABS_ROOT [SCALE] | --replay ABS_ROOT | --restore-drill ABS_ROOT | --self-test');
  } catch (error) { console.error(error instanceof Error ? error.message : 'Fixture operation failed'); process.exitCode = 1; }
}
