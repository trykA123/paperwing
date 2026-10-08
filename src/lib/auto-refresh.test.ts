const testModule = 'bun:test';
const { afterEach, beforeEach, describe, expect, jest, test } = await import(testModule);
import { AutoRefresh, REFRESH_WINDOW_MS, type AutoRefreshHost } from './auto-refresh';
import { watchTargets } from './watch-targets';
import type { LocalStatus, RepoSet, SetItem } from './api';

function fixture(overrides: Partial<AutoRefreshHost> = {}) {
  const refreshed: string[][] = [];
  const notices: string[] = [];
  const watched = new Map<string, string[]>();
  const host: AutoRefreshHost = {
    refresh: paths => { refreshed.push(paths); },
    notice: message => { notices.push(message); },
    watch: async (setId, roots) => { watched.set(setId, roots); },
    unwatch: async setId => { watched.delete(setId); },
    ...overrides,
  };
  return { refresh: new AutoRefresh(host), refreshed, notices, watched };
}

describe('refresh window', () => {
  beforeEach(() => { jest.useFakeTimers(); });
  afterEach(() => { jest.useRealTimers(); });

  test('two events inside the window cause one refresh of the affected rows', () => {
    const { refresh, refreshed } = fixture();
    refresh.changed('/work/a');
    jest.advanceTimersByTime(REFRESH_WINDOW_MS - 100);
    refresh.changed('/work/b');
    refresh.changed('/work/a');
    jest.advanceTimersByTime(REFRESH_WINDOW_MS);
    expect(refreshed).toEqual([['/work/a', '/work/b']]);
  });

  test('an event after the window starts a new refresh', () => {
    const { refresh, refreshed } = fixture();
    refresh.changed('/work/a');
    jest.advanceTimersByTime(REFRESH_WINDOW_MS);
    refresh.changed('/work/a');
    jest.advanceTimersByTime(REFRESH_WINDOW_MS);
    expect(refreshed).toEqual([['/work/a'], ['/work/a']]);
  });
});

describe('watch lifecycle', () => {
  test('closing a set, removing its last repository and closing the app return the count to zero', async () => {
    const { refresh, watched } = fixture();
    await refresh.sync(new Map([['a', ['/w/1', '/w/2']], ['b', ['/w/3']]]));
    expect(refresh.watching).toBe(2);
    await refresh.sync(new Map([['a', ['/w/1']]]));
    expect([...watched.keys()]).toEqual(['a']);
    expect(watched.get('a')).toEqual(['/w/1']);
    await refresh.sync(new Map([['a', []]]));
    expect(refresh.watching).toBe(0);
    expect(watched.size).toBe(0);
  });

  test('an unchanged target list does not restart the watch', async () => {
    let starts = 0;
    const { refresh } = fixture({ watch: async () => { starts += 1; } });
    await refresh.sync(new Map([['a', ['/w/1', '/w/2']]]));
    await refresh.sync(new Map([['a', ['/w/2', '/w/1']]]));
    expect(starts).toBe(1);
  });
});

describe('failure path', () => {
  test('a refused watch shows one notice and keeps the count at zero', async () => {
    const { refresh, notices } = fixture({ watch: async () => { throw 'This set has 201 repositories; automatic refresh watches up to 200.'; } });
    await refresh.sync(new Map([['a', ['/w/1']], ['b', ['/w/2']]]));
    expect(refresh.watching).toBe(0);
    expect(notices).toEqual(['This set has 201 repositories; automatic refresh watches up to 200. Refresh local status still works.']);
  });

  test('an injected watcher error stops that watch and shows one notice', async () => {
    const { refresh, notices, watched } = fixture();
    await refresh.sync(new Map([['a', ['/w/1']]]));
    await refresh.failed('a', 'inotify queue overflowed');
    await refresh.failed('a', 'inotify queue overflowed');
    expect(refresh.watching).toBe(0);
    expect(watched.size).toBe(0);
    expect(notices).toEqual(['Automatic refresh stopped: inotify queue overflowed. Refresh local status still works.']);
  });

  test('a later change of roots tries again and a new failure is announced again', async () => {
    let refuse = true;
    const { refresh, notices } = fixture({ watch: async () => { if (refuse) throw 'boom'; } });
    await refresh.sync(new Map([['a', ['/w/1']]]));
    refuse = false;
    await refresh.sync(new Map([['a', ['/w/1', '/w/2']]]));
    expect(refresh.watching).toBe(1);
    refuse = true;
    await refresh.sync(new Map([['a', ['/w/3']]]));
    expect(notices).toHaveLength(2);
  });
});

describe('watch targets', () => {
  const item = (id: string): SetItem => ({ id, repoId: id, url: '', org: '', name: id, ref: { type: 'branch', name: 'main' }, on: false });
  const sets: RepoSet[] = [{ id: 'a', name: 'A', items: [item('1'), item('2')] }, { id: 'b', name: 'B', items: [item('3')] }];
  const local = { '/w/1': { repo: true }, '/w/2': { repo: false }, '/w/3': { repo: true } } as unknown as Record<string, LocalStatus>;
  const dest = (entry: SetItem) => `/w/${entry.id}`;

  test('only cloned repositories of sets with an open tab are watched', () => {
    const tabs = [{ setId: 'a', view: { kind: 'set' } }, { setId: 'b', view: { kind: 'settings' } }];
    expect(watchTargets({ sets, tabs, local, dest })).toEqual(new Map([['a', ['/w/1']]]));
  });

  test('the home list watches every set', () => {
    const tabs = [{ setId: 'a', view: { kind: 'repos' } }];
    expect(watchTargets({ sets, tabs, local, dest })).toEqual(new Map([['a', ['/w/1']], ['b', ['/w/3']]]));
  });

  test('no tabs watch nothing', () => {
    expect(watchTargets({ sets, tabs: [], local, dest }).size).toBe(0);
  });
});
