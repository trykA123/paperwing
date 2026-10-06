import { expect, test } from 'bun:test';
import { withIpc } from './test-support/ipc-fixture';
import { api } from './api';

test('pull wrappers pass branch and request values through IPC', async () => {
  const calls = [];
  await withIpc((command, args) => {
    calls.push({ command, args });
    if (command === 'pull_for_branch') return null;
    return { number: 28, url: 'https://gitext.company.com/admin/repo/pull/28', targetRepo: 'admin/repo', hasUnpushedCommits: true };
  }, async () => {
    expect(await api.pullForBranch('/workspace/repo', 'feature/login')).toBeNull();
    const request = { head: 'feature/login', base: 'main', title: 'Login', body: '' };
    expect(await api.openPullRequest('/workspace/repo', request)).toEqual({ number: 28, url: 'https://gitext.company.com/admin/repo/pull/28', targetRepo: 'admin/repo', hasUnpushedCommits: true });
    expect(calls).toEqual([
      { command: 'pull_for_branch', args: { path: '/workspace/repo', branch: 'feature/login' } },
      { command: 'open_pull_request', args: { path: '/workspace/repo', request } },
    ]);
  });
});


test('pull wrappers preserve typed rate limit errors', async () => {
  const error = { kind: 'rateLimited', resetAt: '2026-10-06T16:00:00+00:00', message: 'Retry later' };
  await withIpc(() => { throw error; }, async () => {
    await expect(api.pullForBranch('/workspace/repo', 'feature/login')).rejects.toEqual(error);
    await expect(api.openPullRequest('/workspace/repo', { head: 'feature/login', base: 'main', title: 'Login', body: '' })).rejects.toEqual(error);
  });
});
