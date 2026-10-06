import { strict as assert } from 'node:assert';
import { randomUUID } from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { markedRoot, newRoot, noSymlinks, replay, createFixtures, restoreDrill } from './fixtures';

export const profileMarker = 'paperwing-disposable-profile-v1\n';
export const identifier = 'dev.paperwing.testing';
const required = ['config/' + identifier, 'data/' + identifier, 'cache/' + identifier, 'data/' + identifier + '/logs', 'webview'];

export function prepareProfile(profile: string, fixtures: string) {
  const fixture = replay(fixtures);
  restoreDrill(fixtures);
  newRoot(profile, profileMarker);
  for (const path of required) mkdirSync(join(profile, path), { recursive: true });
  const sourceId = 'fixture-' + randomUUID();
  const setId = 'fixture-set', itemId = 'fixture-item';
  const settings = {
    sources: [{ id: sourceId, name: 'Disposable fixture', kind: 'manual', host: '', orgs: [], urls: [] }],
    workspace: { root: join(markedRoot(fixtures), 'checkouts'), layout: 'flat', activeSet: setId, sets: [{ id: setId, name: 'Disposable fixture', items: [{ id: itemId, repoId: sourceId, url: join(fixtures, 'source.git'), org: '', name: 'right', folder: 'right', ref: { type: 'branch', name: 'main' }, on: true }] }] },
  };
  const endpoint = { setId, itemId };
  const plan = { left: { ...endpoint, reference: { kind: 'commit', sha: fixture.base } }, right: { ...endpoint, reference: { kind: 'commit', sha: fixture.head } } };
  writeFileSync(join(profile, 'config', identifier, 'settings.json'), JSON.stringify(settings) + '\n', { flag: 'wx' });
  writeFileSync(join(profile, 'benchmark-plan.json'), JSON.stringify(plan) + '\n', { flag: 'wx' });
  writeFileSync(join(profile, 'profile.json'), JSON.stringify({ version: 1, identifier, fixtures: markedRoot(fixtures), content: fixture.content, objects: fixture.objects }) + '\n', { flag: 'wx' });
  return validateProfile(profile);
}

export function validateProfile(profile: string) {
  const root = markedRoot(profile, profileMarker);
  for (const path of required) {
    const directory = noSymlinks(join(root, path));
    if (!statSync(directory).isDirectory()) throw new Error('Profile directory is missing');
  }
  const metadata = JSON.parse(readFileSync(noSymlinks(join(root, 'profile.json')), 'utf8'));
  if (metadata.version !== 1 || metadata.identifier !== identifier) throw new Error('Profile identifier mismatch');
  const fixture = replay(metadata.fixtures);
  if (metadata.content !== fixture.content || metadata.objects !== fixture.objects) throw new Error('Profile fixture mismatch');
  const settings = JSON.parse(readFileSync(noSymlinks(join(root, 'config', identifier, 'settings.json')), 'utf8'));
  if (settings.sources.length !== 1 || !/^fixture-[0-9a-f-]{36}$/.test(settings.sources[0].id)) throw new Error('Fixture source identity mismatch');
  if (settings.workspace.root !== join(metadata.fixtures, 'checkouts')) throw new Error('Profile workspace escaped fixtures');
  noSymlinks(settings.workspace.root);
  return { root, metadata };
}

export function profileEnvironment(profile: string, sample = 1) {
  const { root } = validateProfile(profile);
  if (!Number.isSafeInteger(sample) || sample < 1 || sample > 0xffffffff) throw new Error('Sample must be a positive u32 integer');
  const environment = { ...process.env };
  for (const name of Object.keys(environment)) if (name.startsWith('GIT_')) delete environment[name];
  return { ...environment, PAPERWING_TEST_PROFILE: root, PAPERWING_BENCHMARK_SAMPLE: String(sample), XDG_CONFIG_HOME: join(root, 'config'), XDG_DATA_HOME: join(root, 'data'), XDG_CACHE_HOME: join(root, 'cache'), GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: '/dev/null', GIT_CONFIG_COUNT: '1', GIT_CONFIG_KEY_0: 'core.attributesFile', GIT_CONFIG_VALUE_0: '/dev/null', GIT_ATTR_NOSYSTEM: '1', GIT_TERMINAL_PROMPT: '0' };
}

export function requireDisplay(environment = process.env) {
  if (process.platform !== 'linux') throw new Error('This runner currently supports Linux only');
  if (!environment.DISPLAY && !environment.WAYLAND_DISPLAY) throw new Error('Native measurement unavailable: DISPLAY and WAYLAND_DISPLAY are unset. Run --prepare and --launch from a desktop session.');
}

export function processTree(pid: number) {
  const seen = new Set<number>();
  let rssBytes = 0, missedReads = 0;
  const read = (current: number) => {
    if (seen.has(current)) return;
    seen.add(current);
    try {
      const status = readFileSync(`/proc/${current}/status`, 'utf8');
      const rss = status.match(/^VmRSS:\s+(\d+)\s+kB$/m);
      if (rss) rssBytes += Number(rss[1]) * 1024;
      const children = new Set<number>();
      for (const thread of readdirSync(`/proc/${current}/task`)) {
        try {
          const value = readFileSync(`/proc/${current}/task/${thread}/children`, 'utf8').trim();
          if (value) for (const child of value.split(/\s+/)) children.add(Number(child));
        } catch { missedReads++; }
      }
      for (const child of children) read(child);
    } catch { missedReads++; }
  };
  read(pid);
  return { rssBytes, processes: seen.size, missedReads };
}

export async function launch(profile: string, binary: string, sample = 1) {
  requireDisplay();
  const environment = profileEnvironment(profile, sample);
  const artifact = noSymlinks(resolve(binary));
  if (!statSync(artifact).isFile()) throw new Error('Native release binary missing');
  if (!readFileSync(artifact).includes(Buffer.from('paperwing-test-profile-build-v1'))) throw new Error('Refusing artifact without test-profile build marker');
  const inspection = spawnSync(artifact, ['--paperwing-test-profile-info'], { env: environment, encoding: 'utf8', timeout: 10_000, stdio: ['ignore', 'pipe', 'ignore'] });
  if (inspection.error || inspection.status !== 0) throw new Error('Test-profile artifact inspection failed');
  const capabilities = JSON.parse(inspection.stdout);
  if (capabilities.profileBuild !== 'paperwing-test-profile-build-v1' || capabilities.identifier !== identifier || capabilities.benchmark !== true) throw new Error('Artifact identifier/features do not match isolated profile');
  const trace = join(profile, 'data', identifier, 'logs', `sample-${sample}.jsonl`);
  if (existsSync(trace)) throw new Error('Refusing existing sample trace');
  const webview = noSymlinks(join(profile, 'webview', `sample-${sample}`));
  if (existsSync(webview)) throw new Error('Refusing reused WebView sample directory');
  if (readdirSync(join(profile, 'cache', identifier)).length) throw new Error('Application cache is not empty; no cache-empty measurement accepted');
  mkdirSync(webview);
  const child = spawn(artifact, [], { env: environment, timeout: 180_000, stdio: ['ignore', 'ignore', 'ignore'] });
  const memory = { sample, applicationCache: 'app-cache-empty', webviewCache: 'empty-new-sample-directory', osCache: 'uncontrolled', peakRssBytes: 0, maxProcesses: 0, missedReads: 0, polls: 0, intervalMs: 50, method: 'Sampled Linux process-tree RSS sum; shared pages may be counted more than once; transient peaks may be missed' };
  const poll = () => {
    if (!child.pid) return;
    const current = processTree(child.pid);
    memory.peakRssBytes = Math.max(memory.peakRssBytes, current.rssBytes);
    memory.maxProcesses = Math.max(memory.maxProcesses, current.processes);
    memory.missedReads += current.missedReads;
    memory.polls++;
  };
  poll();
  const interval = setInterval(poll, memory.intervalMs);
  try {
    await new Promise<void>((resolve, reject) => {
      child.once('error', () => reject(new Error('Native sample could not start')));
      child.once('exit', (code, signal) => code === 0 && !signal ? resolve() : reject(new Error('Native sample failed or timed out; no measurement accepted')));
    });
  } finally { clearInterval(interval); }
  if (!existsSync(trace)) throw new Error('No instrumented trace: verify test-profile feature and testing configuration');
  if (!existsSync(trace.replace(/\.jsonl$/, '.result.json'))) throw new Error('Native sample did not record a completed result');
  if (!memory.polls || !memory.peakRssBytes) throw new Error('Process-tree memory could not be measured');
  writeFileSync(trace.replace(/\.jsonl$/, '.system.json'), JSON.stringify(memory) + '\n', { flag: 'wx' });
  validateProfile(profile);
  return trace;
}

async function nonleaderChildTest() {
  const program = `import json, signal, subprocess, sys, threading
done = threading.Event()
ready = threading.Event()
child = None
def worker():
    global child
    child = subprocess.Popen([sys.executable, "-u", "-c", "import time; payload = bytearray(16 * 1024 * 1024); print('ready', flush=True); time.sleep(120)"], stdout=subprocess.PIPE, text=True)
    try:
        child.stdout.readline()
        ready.set()
        done.wait()
    finally:
        child.terminate()
        child.wait()
thread = threading.Thread(target=worker)
thread.start()
signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
try:
    ready.wait()
    print(json.dumps({"child": child.pid}), flush=True)
    sys.stdin.read(1)
finally:
    done.set()
    thread.join()
`;
  const parent = spawn('python3', ['-u', '-c', program], { stdio: ['pipe', 'pipe', 'ignore'] });
  const exited = new Promise<void>((resolve, reject) => {
    parent.once('exit', code => code === 0 ? resolve() : reject(new Error('Multithreaded memory fixture failed')));
    parent.once('error', reject);
  });
  const timeout = setTimeout(() => parent.kill('SIGTERM'), 10_000);
  try {
    const child = await new Promise<number>((resolve, reject) => {
      let output = '';
      parent.stdout.on('data', value => {
        output += value.toString();
        if (output.includes('\n')) {
          try { resolve(JSON.parse(output.trim()).child); } catch { reject(new Error('Invalid memory fixture readiness')); }
        }
      });
      parent.once('exit', () => reject(new Error('Memory fixture ended before readiness')));
      parent.once('error', reject);
    });
    assert.ok(parent.pid);
    const leaderChildren = readFileSync(`/proc/${parent.pid}/task/${parent.pid}/children`, 'utf8').trim().split(/\s+/).map(Number);
    assert.ok(!leaderChildren.includes(child));
    const parentStatus = readFileSync(`/proc/${parent.pid}/status`, 'utf8');
    const parentRss = Number(parentStatus.match(/^VmRSS:\s+(\d+)\s+kB$/m)![1]) * 1024;
    const tree = processTree(parent.pid!);
    assert.equal(tree.processes, 2);
    assert.ok(tree.rssBytes > parentRss + 8 * 1024 * 1024);
  } finally {
    parent.stdin.end('q');
    await exited;
    clearTimeout(timeout);
  }
}

async function selfTest() {
  const parent = mkdtempSync(join(tmpdir(), 'paperwing-profile-self-test-'));
  const fixtures = join(parent, 'fixtures'), profile = join(parent, 'profile');
  createFixtures(fixtures, 8);
  prepareProfile(profile, fixtures);
  const environment = profileEnvironment(profile);
  assert.equal(environment.XDG_CONFIG_HOME, join(profile, 'config'));
  assert.equal(environment.XDG_DATA_HOME, join(profile, 'data'));
  assert.equal(environment.XDG_CACHE_HOME, join(profile, 'cache'));
  assert.equal(environment.HOME, process.env.HOME);
  assert.equal(environment.GIT_ATTR_NOSYSTEM, '1');
  assert.equal(environment.GIT_CONFIG_COUNT, '1');
  assert.equal(environment.GIT_CONFIG_KEY_0, 'core.attributesFile');
  assert.equal(environment.GIT_CONFIG_VALUE_0, '/dev/null');
  assert.throws(() => prepareProfile(profile, fixtures));
  assert.throws(() => validateProfile(fixtures));
  assert.throws(() => requireDisplay({}));
  const link = join(parent, 'linked'); symlinkSync(profile, link);
  assert.throws(() => validateProfile(link));
  assert.throws(() => profileEnvironment(profile, NaN));
  const settingsPath = join(profile, 'config', identifier, 'settings.json');
  const settings = JSON.parse(readFileSync(settingsPath, 'utf8'));
  settings.sources[0].id = 'real-source';
  writeFileSync(settingsPath, JSON.stringify(settings));
  assert.throws(() => validateProfile(profile));
  await nonleaderChildTest();
  console.log('native-profile self-test passed, including nonleader-thread child RSS; native launch not performed');
}

if (import.meta.main) {
  try {
    const [command, root, target, sample] = process.argv.slice(2);
    if (command === '--self-test') await selfTest();
    else if (command === '--prepare' && root && target) {
      prepareProfile(root, target);
      console.log('Isolated profile prepared. From a Linux desktop session:');
      console.log('bun run --bun tauri build --no-bundle --config src-tauri/tauri.test.conf.json --features test-profile -- --offline --locked');
      console.log('bun run scripts/testing/native-profile.ts --launch ABS_PROFILE ABS_RELEASE_BINARY [SAMPLE]');
      console.log('bun run scripts/testing/baseline.ts --report ABS_PROFILE --cache app-cache-empty --os-cache uncontrolled');
    } else if (command === '--launch' && root && target) console.log(await launch(root, target, sample === undefined ? 1 : Number(sample)));
    else throw new Error('Usage: native-profile.ts --prepare ABS_PROFILE ABS_FIXTURES | --launch ABS_PROFILE ABS_BINARY [SAMPLE] | --self-test');
  } catch (error) { console.error(error instanceof Error ? error.message : 'Native profile operation failed'); process.exitCode = 1; }
}
