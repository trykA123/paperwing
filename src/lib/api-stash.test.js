import { expect, test } from 'bun:test';
import { withIpc } from './test-support/ipc-fixture';
import { api } from './api';

test('stash wrappers send the command name and arguments', async () => {
  const calls = [];
  await withIpc((command, args) => { calls.push({ command, args }); return null; }, async () => {
    await api.stashList('/r');
    await api.stashPush('/r', 'wip', true);
    await api.stashApply('/r', 'abc');
    await api.stashPop('/r', 'abc');
    await api.stashDrop('/r', 'abc');
    await api.stashShow('/r', 'abc');
    await api.switchWithStash('/r', 'release');
  });
  expect(calls).toEqual([
    { command: 'stash_list', args: { path: '/r' } },
    { command: 'stash_push', args: { path: '/r', message: 'wip', includeUntracked: true } },
    { command: 'stash_apply', args: { path: '/r', oid: 'abc' } },
    { command: 'stash_pop', args: { path: '/r', oid: 'abc' } },
    { command: 'stash_drop', args: { path: '/r', oid: 'abc' } },
    { command: 'stash_show', args: { path: '/r', oid: 'abc' } },
    { command: 'switch_with_stash', args: { path: '/r', branch: 'release' } },
  ]);
});
