import './test-support/svelte-loader.js';
import { describe, expect, test } from 'bun:test';
import { defaultSelection, deleteLocalAcross, expectedTips, localRows, summarizeCleanup } from './branch-cleanup.ts';

const { CleanupSession } = await import('./branch-cleanup.svelte.ts');

const branch = (name, extra = {}) => ({ name, oid: `oid-${name}`, merged: true, upstream: null, upstreamGone: false, inWorktree: false, lastCommit: 1700000000, subject: `work on ${name}`, ...extra });
const listing = (extra = {}) => ({
  base: 'origin/main', baseName: 'main', remote: 'origin', remoteBase: 'origin/main', current: 'dev',
  local: [branch('done'), branch('gone', { upstream: 'origin/gone', upstreamGone: true }), branch('wt', { inWorktree: true }), branch('wip', { merged: false }), branch('dev'), branch('main')],
  remoteBranches: [{ name: 'old', oid: 'r-old', lastCommit: 1, subject: 's' }, { name: 'main', oid: 'r-main', lastCommit: 1, subject: 's' }],
  ...extra,
});

describe('selection defaults', () => {
  test('preselects only merged branches that are not current, protected or in a worktree', () => {
    expect(defaultSelection(localRows(listing()))).toEqual(['done']);
  });

  test('marks upstream-gone branches without preselecting them', () => {
    const row = localRows(listing()).find(entry => entry.name === 'gone');
    expect(row.notes).toEqual(['Upstream gone']);
    expect(row.blocked).toBeNull();
    expect(row.preselect).toBe(false);
  });

  test('never makes the current, protected, worktree or unmerged branches selectable', () => {
    const blocked = Object.fromEntries(localRows(listing()).map(row => [row.name, row.blocked]));
    expect(blocked).toMatchObject({ dev: 'Current branch', main: 'Protected', wt: 'Checked out in a worktree', wip: 'Not merged into origin/main', done: null });
  });

});

describe('deleting', () => {
  const target = name => ({ path: `/r/${name}`, name });
  const entry = (name, names) => { const data = listing(); return { target: target(name), data, rows: localRows(data), names }; };

  test('passes each branch with its expected tip and the resolved base', async () => {
    const calls = [];
    const client = { deleteMergedBranches: async (...args) => { calls.push(args); return args[1].map(name => ({ name, deleted: true, error: null })); } };
    await deleteLocalAcross(client, [entry('a', ['done', 'gone'])]);
    expect(calls).toEqual([['/r/a', ['done', 'gone'], ['oid-done', 'oid-gone'], 'origin/main']]);
  });

  test('drops blocked names before calling the backend', async () => {
    const calls = [];
    const client = { deleteMergedBranches: async (...args) => { calls.push(args[1]); return []; } };
    await deleteLocalAcross(client, [entry('a', ['dev', 'main', 'wt', 'wip', 'done'])]);
    expect(calls).toEqual([['done']]);
  });

  test('one failing repository among three does not stop the others', async () => {
    const client = {
      deleteMergedBranches: async (path, names) => {
        if (path === '/r/b') throw new Error('locked');
        return names.map(name => ({ name, deleted: path !== '/r/c', error: path === '/r/c' ? 'tip moved' : null }));
      },
    };
    const results = await deleteLocalAcross(client, [entry('a', ['done']), entry('b', ['done']), entry('c', ['done'])]);
    expect(results.map(result => [result.target.name, result.deleted, result.failed.map(item => item.error), result.error])).toEqual([
      ['a', ['done'], [], null], ['b', [], [], 'Could not delete branches in b: locked'], ['c', [], ['tip moved'], null],
    ]);
    expect(summarizeCleanup(results)).toEqual({ deleted: 1, failed: 2 });
  });
});

describe('session', () => {
  test('loads every repository, keeps a failing listing apart and deletes only the selection', async () => {
    const deleted = [];
    const client = {
      mergedBranches: async path => { if (path === '/r/bad') throw new Error('not a repository'); return listing(); },
      deleteMergedBranches: async (path, names) => { deleted.push([path, names]); return names.map(name => ({ name, deleted: true, error: null })); },
    };
    const session = new CleanupSession([{ path: '/r/a', name: 'a' }, { path: '/r/bad', name: 'bad' }, { path: '/r/c', name: 'c' }], client);
    await session.load();
    expect(session.repos.map(repo => repo.load.status)).toEqual(['ready', 'error', 'ready']);
    expect(session.localCount).toBe(2);
    session.toggle('/r/c', 'done', false);
    await session.run();
    expect(deleted).toEqual([['/r/a', ['done']]]);
    expect(session.repos[0].result.deleted).toEqual(['done']);
    session.setAll('/r/a', false);
    await session.run();
    expect(session.repos.map(repo => repo.result)).toEqual([null, null, null]);
  });
});
