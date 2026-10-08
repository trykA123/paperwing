import { expect, test } from 'bun:test';
import { partialStagingApi } from './api';
import { withIpc } from './test-support/ipc-fixture';

test('partial staging wrappers send snapshots, line ranges and discard confirmation', async () => {
  const calls = [];
  const request = {
    file: 'with spaces.txt', origPath: null, area: 'unstaged', contentHash: 'snapshot',
    hunks: [{ hunk: 1, ranges: [{ start: 2, end: 4 }] }],
  };
  const files = [{ file: request.file, contentHash: request.contentHash }];
  await withIpc((command, args) => {
    calls.push({ command, args });
    return null;
  }, async () => {
    await partialStagingApi.changeHunks('/repo', request.file, null, 'unstaged');
    await partialStagingApi.stageHunks('/repo', request);
    await partialStagingApi.unstageHunks('/repo', { ...request, area: 'staged' });
    await partialStagingApi.discardFiles('/repo', files, false);
    await partialStagingApi.discardHunk('/repo', { ...request, hunks: [{ hunk: 1, ranges: null }] }, true);
  });
  expect(calls).toEqual([
    { command: 'change_hunks', args: { path: '/repo', file: request.file, origPath: null, area: 'unstaged' } },
    { command: 'stage_hunks', args: { path: '/repo', request } },
    { command: 'unstage_hunks', args: { path: '/repo', request: { ...request, area: 'staged' } } },
    { command: 'discard_files', args: { path: '/repo', files, confirmed: false } },
    { command: 'discard_hunk', args: { path: '/repo', request: { ...request, hunks: [{ hunk: 1, ranges: null }] }, confirmed: true } },
  ]);
});

test('discard wrappers preserve Recovery and operating-system Trash results', async () => {
  const outcomes = [
    { file: 'tracked.txt', state: 'discarded', recoveryId: 'record', message: 'Undo in Recovery', warning: 'Linux contract' },
    { file: 'new.txt', state: 'trashed', recoveryId: null, message: 'Moved to the desktop Trash', warning: null },
    { file: 'blocked.txt', state: 'failed', recoveryId: null, message: 'Recovery storage is full', warning: null },
  ];
  await withIpc(() => outcomes, async () => {
    expect(await partialStagingApi.discardFiles('/repo', [{ file: 'tracked.txt', contentHash: 'snapshot' }], true)).toEqual(outcomes);
  });
});
