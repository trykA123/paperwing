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

test('a failed chunk does not stop the others and the error surfaces at the end', async () => {
  await statusFixture(16, async (state, calls, paths) => {
    const checking = state.checkExists(paths).then(() => 'done', reason => String(reason));
    const [first, second] = statusCalls(calls);
    first.reject('status failed');
    second.resolve(second.args.paths.map(row));
    expect(await checking).toBe('status failed');
    expect(Object.keys(state.local)).toEqual(paths.slice(8));
  });
});
