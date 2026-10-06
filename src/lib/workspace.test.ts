const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import type { Workspace } from './api';
import { defaultWorkspace, migrateWorkspace } from './workspace';

describe('P1 saved workspace', () => {
  test('legacy data, unknown fields and duplicate folders survive two round trips', () => {
    const legacy = {
      activeSet: 'set-b', root: 'D:\\existing', layout: 'org',
      sets: [{ id: 'set-a', name: 'A', items: [] }, { id: 'set-b', name: 'B', items: [
        { id: 'copy-1', repoId: 'src:repo', name: 'repo', org: 'org', url: 'https://example.test/repo', ref: { type: 'branch', name: 'main' }, on: true },
        { id: 'copy-2', repoId: 'src:repo', name: 'repo', folder: 'repo_2', org: 'org', url: 'https://example.test/repo', ref: { type: 'tag', name: 'v1' }, on: false },
      ] }],
      stars: ['src:repo'], cols: { repo: 275 }, rightWidth: 340,
      shallow: true, parallel: 8, onExisting: 'skip', pageSize: 50,
      theme: 'dark', uiFont: 'plex', codeFont: 'fira', futureProperty: { preserved: true },
    };
    const first = migrateWorkspace(legacy as unknown as Partial<Workspace>);
    const second = migrateWorkspace(JSON.parse(JSON.stringify(first)));
    const third = migrateWorkspace(JSON.parse(JSON.stringify(second)));
    expect(third).toEqual(first);
    expect(third).toMatchObject({ ...legacy, layout: 'custom', cols: { repo: 275 }, shell: { version: 1 } });
    expect(third.sets[1].items.map(item => item.id)).toEqual(['copy-1', 'copy-2']);
    expect(third.sets[1].items[1].folder).toBe('repo_2');
  });

  test('defaults are complete and pane preferences survive reload', () => {
    const defaults = defaultWorkspace();
    expect(defaults.root).toBe('C:\\Dev\\repos');
    expect(defaults.shell).toEqual({ version: 1, sidebarWidth: 250, sidebarVisible: true, rightVisible: true, section: 'sets' });
    defaults.shell = { version: 1, sidebarWidth: 300, sidebarVisible: false, rightVisible: false, section: 'activity' };
    expect(migrateWorkspace(JSON.parse(JSON.stringify(defaults)))).toEqual(defaults);
  });

  test('empty sets and stale active identity recover without changing the root', () => {
    const restored = migrateWorkspace({ sets: [], activeSet: 'deleted', root: 'E:\\private' });
    expect(restored.sets).toHaveLength(1);
    expect(restored.activeSet).toBe(restored.sets[0].id);
    expect(restored.root).toBe('E:\\private');
  });
});