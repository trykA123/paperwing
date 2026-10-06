import { expect, test } from 'bun:test';
import { withIpc } from './test-support/ipc-fixture';
import { api } from './api';

test('pull wrappers pass branch and request values through IPC', async () => {
  const calls = [];
  await withIpc((command, args) => {
    calls.push({ command, args });
    if (command === 'pull_for_branch') return null;
    return { number: 28, url: 'https://github.com/admin/repo/pull/28' };
  }, async () => {
    expect(await api.pullForBranch('/workspace/repo', 'feature/login')).toBeNull();
    const request = { head: 'feature/login', base: 'main', title: 'Login', body: '' };
    expect(await api.openPullRequest('/workspace/repo', request)).toEqual({ number: 28, url: 'https://github.com/admin/repo/pull/28' });
    expect(calls).toEqual([
      { command: 'pull_for_branch', args: { path: '/workspace/repo', branch: 'feature/login' } },
      { command: 'open_pull_request', args: { path: '/workspace/repo', request } },
    ]);
  });
});
