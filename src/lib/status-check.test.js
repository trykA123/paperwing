import './test-support/svelte-loader.js';
import { expect, test } from 'bun:test';
import { deferred, withIpc } from './test-support/ipc-fixture.js';

const { app } = await import('./state.svelte.ts');

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
