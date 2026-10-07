const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import type { Workspace } from './api';
import { isRepoSection, REPO_SECTIONS, sectionCount, usableSection } from './repo-sections';
import { migrateWorkspace, tabId } from './workspace';

describe('repository page sections', () => {
  test('eight sections, Overview first, Changes History Stash Pull requests and Compare need a clone', () => {
    expect(REPO_SECTIONS.map(section => section.id)).toEqual(['overview', 'changes', 'history', 'branches', 'stash', 'prs', 'actions', 'compare']);
    expect(REPO_SECTIONS.filter(section => section.needsClone).map(section => section.id)).toEqual(['changes', 'history', 'stash', 'prs', 'compare']);
  });

  test('a remote-only repository falls back to Overview for a section that needs a clone', () => {
    expect(usableSection('history', false)).toBe('overview');
    expect(usableSection('branches', false)).toBe('branches');
    expect(usableSection('history', true)).toBe('history');
  });

  test('counts come from what is loaded and are null while unknown', () => {
    const counts = { changes: 3, branches: null, stash: 2, prs: 1 };
    expect(sectionCount('changes', counts)).toBe(3);
    expect(sectionCount('branches', counts)).toBeNull();
    expect(sectionCount('overview', counts)).toBeNull();
  });

  test('a page has one tab per repository, whatever its section', () => {
    expect(tabId({ kind: 'repo', repoId: 's:o/r', section: 'history' }, 'a')).toBe(tabId({ kind: 'repo', repoId: 's:o/r', section: 'stash' }, 'b'));
    expect(tabId({ kind: 'repo', repoId: 's:o/r', section: 'history' }, 'a')).not.toBe(tabId({ kind: 'repo', repoId: 's:o/x', section: 'history' }, 'a'));
  });

  test('the saved last repository survives a reload and a damaged one is dropped', () => {
    const shell = (lastRepo: unknown) => ({ shell: { version: 1, sidebarWidth: 250, sidebarVisible: true, rightVisible: true, section: 'repos', lastRepo } }) as unknown as Partial<Workspace>;
    expect(migrateWorkspace(shell({ repoId: 's:o/r', section: 'stash' })).shell.lastRepo).toEqual({ repoId: 's:o/r', section: 'stash' });
    expect(migrateWorkspace(shell({ repoId: 's:o/r', section: 'nowhere' })).shell.lastRepo).toBeUndefined();
    expect(isRepoSection('stash')).toBe(true);
  });
});

describe('an older saved workspace', () => {
  test('with a right panel, a width and the sets section loads into Repositories without losing anything else', () => {
    const saved = {
      sets: [{ id: 's', name: 'Release', items: [{ id: 'i', repoId: 'a:o/r', url: 'u', org: 'o', name: 'r', ref: { type: 'branch', name: 'main' }, on: true }] }], activeSet: 's', stars: ['a:o/r'], rightWidth: 420, pageSize: 50,
      shell: { version: 1, sidebarWidth: 300, sidebarVisible: false, rightVisible: false, section: 'sets' },
    } as unknown as Partial<Workspace>;
    const loaded = migrateWorkspace(saved);
    expect(loaded.shell).toMatchObject({ section: 'repos', sidebarWidth: 300, sidebarVisible: false });
    expect(loaded).toMatchObject({ rightWidth: 420, pageSize: 50, stars: ['a:o/r'], activeSet: 's' });
    expect(loaded.sets[0].items[0]).toMatchObject({ id: 'i', on: true });
  });
});
