const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import type { SetItem, Workspace } from './api';
import { cleanScope, scopeItems, scopeLabel, scopeOf, takeUnrequested, toggleScope, unreadPaths } from './scope';
import { defaultWorkspace, migrateWorkspace } from './workspace';

const item = (id: string): SetItem => ({ id, repoId: `src:${id}`, name: id, org: 'o', url: `https://example.test/${id}`, ref: { type: 'branch', name: 'main' }, on: true });
const dest = (entry: SetItem) => `/dev/${entry.name}`.toLowerCase();

describe('scope', () => {
  test('pull requests and actions default to the active set', () => {
    expect(scopeOf({}, 'prs')).toBe('set');
    expect(scopeOf({ scope: { prs: 'all' } }, 'actions')).toBe('set');
  });

  test('toggling one module leaves the other alone', () => {
    const shell: { scope?: Record<string, string> } = {};
    expect(toggleScope(shell, 'prs')).toBe('all');
    expect(shell.scope).toEqual({ prs: 'all' });
    expect(toggleScope(shell, 'actions')).toBe('all');
    expect(toggleScope(shell, 'prs')).toBe('set');
    expect(shell.scope).toEqual({ prs: 'set', actions: 'all' });
  });

  test('all repositories lists each folder once across sets', () => {
    const a = { items: [item('x'), item('y')] };
    const b = { items: [item('y'), item('z')] };
    expect(scopeItems('set', a, [a, b], dest).map(entry => entry.name)).toEqual(['x', 'y']);
    expect(scopeItems('all', a, [a, b], dest).map(entry => entry.name)).toEqual(['x', 'y', 'z']);
  });

  test('folders that differ only in case count once', () => {
    const a = { items: [item('Api')] };
    const b = { items: [item('api')] };
    expect(scopeItems('all', a, [a, b], dest)).toHaveLength(1);
  });

  test('an item of another set without a status entry is read, failed ones are not retried', () => {
    const other = { items: [item('far'), item('near'), item('broken')] };
    const paths = scopeItems('all', { items: [] }, [{ items: [] }, other], dest).map(dest);
    expect(unreadPaths(paths, { '/dev/near': {} }, { '/dev/broken': 'x' })).toEqual(['/dev/far']);
  });

  test('two publishes do not re-request folders still in flight', () => {
    const requested = new Set<string>();
    expect(takeUnrequested(['/a', '/b', '/c'], requested)).toEqual(['/a', '/b', '/c']);
    expect(takeUnrequested(['/b', '/c'], requested)).toEqual([]);
    expect(takeUnrequested(['/c', '/d'], requested)).toEqual(['/d']);
  });

  test('chip words name the set or all repositories', () => {
    expect(scopeLabel('set', 'Platform')).toBe('In Platform');
    expect(scopeLabel('all', 'Platform')).toBe('All repositories');
  });

  test('unknown modules and modes are dropped', () => {
    expect(cleanScope({ prs: 'all', actions: 'nope', jira: 'all' })).toEqual({ prs: 'all' });
    expect(cleanScope('all')).toEqual({});
  });
});

describe('scope in the saved workspace', () => {
  test('survives a save and reload', () => {
    const saved = defaultWorkspace();
    saved.shell.scope = { prs: 'all', actions: 'set' };
    expect(migrateWorkspace(JSON.parse(JSON.stringify(saved))).shell.scope).toEqual({ prs: 'all', actions: 'set' });
  });

  test('a workspace saved before scopes existed loads with the right panel fields ignored', () => {
    const old = { activeSet: 'a', sets: [{ id: 'a', name: 'A', items: [] }], rightWidth: 420, shell: { version: 1, sidebarWidth: 280, sidebarVisible: true, rightVisible: false, section: 'prs' } };
    const loaded = migrateWorkspace(old as unknown as Partial<Workspace>);
    expect(loaded.shell.scope).toBeUndefined();
    expect(scopeOf(loaded.shell, 'prs')).toBe('set');
    expect(loaded.shell).toMatchObject({ sidebarWidth: 280, section: 'prs' });
    expect(loaded.sets[0].name).toBe('A');
  });

  test('a damaged scope field cannot break loading', () => {
    const damaged = { shell: { version: 1, sidebarWidth: 250, sidebarVisible: true, rightVisible: true, section: 'repos', scope: { prs: 7, actions: 'all' } } };
    expect(migrateWorkspace(damaged as unknown as Partial<Workspace>).shell.scope).toEqual({ actions: 'all' });
  });
});
