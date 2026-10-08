const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import type { LocalStatus } from './api';
import { choosePullKey } from './pull-key';

const cloned = { path: '/dev/api', exists: true, repo: true, branch: 'feature' } as unknown as LocalStatus;
const sources = [{ id: 'gh', enabled: false }, { id: 'ghe' }];

describe('pull key', () => {
  test('a cloned repository on a branch has one', () => {
    expect(choosePullKey({ repoId: 'ghe:o/api' }, '/dev/api', cloned, sources)).toEqual({ path: '/dev/api', branch: 'feature' });
  });

  test('a disabled source has none, so no caller asks for its pull requests', () => {
    expect(choosePullKey({ repoId: 'gh:o/api' }, '/dev/api', cloned, sources)).toBeNull();
  });

  test('an unread or detached folder has none', () => {
    expect(choosePullKey({ repoId: 'ghe:o/api' }, '/dev/api', undefined, sources)).toBeNull();
    expect(choosePullKey({ repoId: 'ghe:o/api' }, '/dev/api', { ...cloned, branch: null } as LocalStatus, sources)).toBeNull();
  });
});
