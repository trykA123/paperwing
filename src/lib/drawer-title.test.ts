const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import type { PullRequest, SetItem } from './api';
import type { DrawerTarget } from './details-drawer.svelte';
import { drawerTitle } from './drawer-title';

const item = { id: 'i', repoId: 's:o/r', url: 'https://h/o/r.git', org: 'o', name: 'r', ref: { type: 'branch', name: 'main' }, on: false } as SetItem;
const pull = { number: 7, title: 'Retry 429', url: 'https://h/pull/7', targetRepo: 'o/r' } as PullRequest;
const facts = { folder: 'r', host: 'h', branch: 'main' };

describe('drawer titles', () => {
  test('a quick look names the repository, its host and organization, and its branch', () => {
    expect(drawerTitle({ kind: 'repository', item }, facts)).toEqual({ title: 'r', sub: 'h / o · main', label: 'Quick look at r' });
  });

  test('a remote-only repository has no branch in its subtitle', () => {
    expect(drawerTitle({ kind: 'repository', item }, { ...facts, branch: null }).sub).toBe('h / o');
  });

  test('a pull request shows its number and title', () => {
    expect(drawerTitle({ kind: 'pull', name: 'r', pull }, facts).title).toBe('#7 Retry 429');
  });

  test('a stash and a history name the repository', () => {
    const stash: DrawerTarget = { kind: 'stash', path: 'p', name: 'r', oid: 'a' };
    expect(drawerTitle(stash, facts)).toMatchObject({ title: 'Stash', sub: 'r' });
    expect(drawerTitle({ kind: 'history', path: 'p', name: 'r' }, facts)).toMatchObject({ title: 'r', sub: 'main' });
  });
});
