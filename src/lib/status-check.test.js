import './test-support/svelte-loader.js';
import { expect, test } from 'bun:test';
import { deferred, withIpc } from './test-support/ipc-fixture.js';

const { app } = await import('./state.svelte.ts');

const settle = () => new Promise(resolve => setImmediate(resolve));
const row = path => ({ path, exists: true, repo: true });

async function statusFixture(count, run) {
  const calls = [];
  await withIpc((command, args) => {
    const response = deferred();
    calls.push({ command, args, ...response });
    return response.promise;
  }, async () => {
    const state = new app.constructor();
    state.ws.root = 'C:\\fixture';
    const items = Array.from({ length: count }, (_, index) => ({
      id: `item-${index}`, repoId: `source:org/repo-${index}`, url: `https://fixture.invalid/org/repo-${index}`,
      org: 'org', name: `repo-${index}`, on: true, ref: { type: 'branch', name: 'main' },
    }));
    state.set.items = items;
    await run(state, calls, items.map(item => state.dest(item)));
  });
}

const statusCalls = calls => calls.filter(call => call.command === 'local_status');

test('a focus-triggered status check keeps the refs entries and refetches nothing', async () => {
  await statusFixture(2, async (state, calls, paths) => {
    const url = state.set.items[0].url;
    state.refs = { [url]: { branches: ['main'], tags: ['v1'] } };
    state.markMetadataStale();
    const checking = state.checkExists(paths);
    expect(state.refs[url]).toEqual({ branches: ['main'], tags: ['v1'], stale: true });
    statusCalls(calls)[0].resolve(paths.map(row));
    await checking;
    expect(state.refs[url]).toEqual({ branches: ['main'], tags: ['v1'], stale: true });
    expect(calls.filter(call => call.command === 'get_refs_many')).toHaveLength(0);
  });
});

test('rows from the first chunk are visible before the second chunk resolves', async () => {
  await statusFixture(16, async (state, calls, paths) => {
    const checking = state.checkExists(paths);
    const [first, second] = statusCalls(calls);
    expect(first.args.paths).toEqual(paths.slice(0, 8));
    expect(second.args.paths).toEqual(paths.slice(8));
    first.resolve(first.args.paths.map(row));
    await settle();
    expect(Object.keys(state.local)).toEqual(paths.slice(0, 8));
    second.resolve(second.args.paths.map(row));
    await checking;
    expect(Object.keys(state.local)).toEqual(paths);
  });
});

test('at most four chunks run at once and the next starts when one finishes', async () => {
  await statusFixture(48, async (state, calls, paths) => {
    const checking = state.checkExists(paths);
    expect(statusCalls(calls)).toHaveLength(4);
    statusCalls(calls)[2].resolve(statusCalls(calls)[2].args.paths.map(row));
    await settle();
    expect(statusCalls(calls)).toHaveLength(5);
    for (let index = 0; index < 6; index++) {
      const pending = statusCalls(calls)[index];
      pending.resolve(pending.args.paths.map(row));
      await settle();
    }
    await checking;
    expect(statusCalls(calls)).toHaveLength(6);
  });
});

test('a newer refresh discards stale chunk rows for its paths and keeps its own', async () => {
  await statusFixture(16, async (state, calls, paths) => {
    const older = state.checkExists(paths);
    const newer = state.checkExists(paths.slice(0, 8));
    const [olderFirst, olderSecond, newerFirst] = statusCalls(calls);
    newerFirst.resolve(paths.slice(0, 8).map(path => ({ ...row(path), branch: 'newer' })));
    await newer;
    olderFirst.resolve(olderFirst.args.paths.map(path => ({ ...row(path), branch: 'older' })));
    olderSecond.resolve(olderSecond.args.paths.map(path => ({ ...row(path), branch: 'older' })));
    await older;
    expect(paths.slice(0, 8).map(path => state.local[path].branch)).toEqual(Array(8).fill('newer'));
    expect(paths.slice(8).map(path => state.local[path].branch)).toEqual(Array(8).fill('older'));
  });
});

test('a failed chunk marks its unset paths unavailable with the reason, keeps the other rows and toasts once', async () => {
  await statusFixture(24, async (state, calls, paths) => {
    const toasts = [];
    state.toast = (message, kind) => toasts.push([message, kind]);
    const checking = state.checkExists(paths);
    const [first, second, third] = statusCalls(calls);
    first.reject('disk offline');
    second.reject('disk offline');
    third.resolve(third.args.paths.map(row));
    const rows = await checking;
    expect(rows.map(found => found.path)).toEqual(paths.slice(16));
    expect(Object.keys(state.local)).toEqual(paths.slice(16));
    expect(Object.keys(state.statusFailures)).toEqual(paths.slice(0, 16));
    expect(state.statusFailures[paths[0]]).toContain('disk offline');
    expect(toasts).toHaveLength(1);
  });
});

test('a failed chunk keeps the earlier row for a path instead of marking it unavailable', async () => {
  await statusFixture(2, async (state, calls, paths) => {
    state.local[paths[0]] = { ...row(paths[0]), branch: 'before' };
    const checking = state.checkExists(paths);
    statusCalls(calls)[0].reject('timed out');
    await checking;
    expect(state.statusFailures[paths[0]]).toBeUndefined();
    expect(state.statusFailures[paths[1]]).toContain('timed out');
    expect(state.local[paths[0]].branch).toBe('before');
  });
});

test('a retry clears the unavailable state when the status arrives', async () => {
  await statusFixture(1, async (state, calls, paths) => {
    state.toast = () => {};
    const first = state.checkExists(paths);
    statusCalls(calls)[0].reject('timed out');
    await first;
    expect(state.statusFailures[paths[0]]).toBeDefined();
    const retry = state.checkExists(paths);
    expect(state.statusFailures[paths[0]]).toBeUndefined();
    statusCalls(calls)[1].resolve([row(paths[0])]);
    expect((await retry).map(found => found.path)).toEqual(paths);
    expect(state.local[paths[0]].exists).toBe(true);
  });
});

test('a superseded sweep skips queued chunks whose paths a newer sweep owns', async () => {
  await statusFixture(48, async (state, calls, paths) => {
    const older = state.checkExists(paths);
    const newer = state.checkExists(paths.slice(32));
    expect(statusCalls(calls)).toHaveLength(6);
    const [first] = statusCalls(calls);
    first.resolve(first.args.paths.map(row));
    await settle();
    expect(statusCalls(calls)).toHaveLength(6);
    for (const call of statusCalls(calls)) call.resolve(call.args.paths.map(row));
    await Promise.all([older, newer]);
    expect(Object.keys(state.local)).toHaveLength(48);
  });
});

test('checkExists returns the rows it published', async () => {
  await statusFixture(3, async (state, calls, paths) => {
    const checking = state.checkExists(paths);
    statusCalls(calls)[0].resolve(paths.map(path => ({ ...row(path), exists: false })));
    const rows = await checking;
    expect(rows.map(found => [found.path, found.exists])).toEqual(paths.map(path => [path, false]));
  });
});

test('a finished fetch run marks that run\'s refs stale but keeps them visible', async () => {
  const { emit } = await import('@tauri-apps/api/event');
  const { mockIPC, clearMocks } = await import('@tauri-apps/api/mocks');
  const { windowsPlatform, supportedRoot } = await import('./test-support/platform-fixture');
  const previousWindow = globalThis.window;
  globalThis.window = { crypto: globalThis.crypto, setTimeout: () => 0 };
  mockIPC(async (command, args) => {
    if (command === 'load_settings') return { sources: [], workspace: null };
    if (command === 'platform_info') return windowsPlatform;
    if (command === 'probe_root') return supportedRoot(args.root);
    if (command === 'path_identities') return args.paths.map(path => ({ path, identity: path, exists: true, reason: null }));
    if (command === 'local_status') return args.paths.map(row);
    if (['activity_snapshot'].includes(command)) return [];
    return null;
  }, { shouldMockEvents: true });
  try {
    app.tabs = [];
    await app.init();
    app.ws.root = 'C:\\fixture';
    const items = [0, 1].map(index => ({ id: `run-${index}`, repoId: `r${index}`, url: `https://fixture.invalid/run-${index}`, org: 'org', name: `run-${index}`, on: true, ref: { type: 'branch', name: 'main' } }));
    app.set.items = items;
    app.refs = Object.fromEntries(items.map(item => [item.url, { branches: ['main'], tags: [] }]));
    await app.startClone([items[0]], 'fetch');
    await emit('clone-finished');
    await settle();
    expect(app.needsRefs(items[0].url)).toBe(true);
    expect(app.refs[items[0].url].branches).toEqual(['main']);
    expect(app.needsRefs(items[1].url)).toBe(false);
  } finally {
    clearMocks();
    if (previousWindow === undefined) delete globalThis.window;
    else globalThis.window = previousWindow;
  }
});
