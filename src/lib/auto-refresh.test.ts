const testModule = 'bun:test';
const { afterEach, beforeEach, describe, expect, jest, test } = await import(testModule);
import { AutoRefresh, REFRESH_MAX_WAIT_MS, REFRESH_TRAILING_MS, type AutoRefreshHost } from './auto-refresh';
import { watchTargets } from './watch-targets';
import type { LocalStatus, RepoSet, SetItem } from './api';

const quiet = { watched: 1, skipped: [], bestEffort: [] };
const settle = async () => { for (let turn = 0; turn < 8; turn += 1) await Promise.resolve(); };

function fixture(overrides: Partial<AutoRefreshHost> = {}) {
  const refreshed: string[][] = [];
  const notices: { message: string; kind: string }[] = [];
  const watched = new Map<string, string[]>();
  const host: AutoRefreshHost = {
    refresh: paths => { refreshed.push(paths); },
    notice: (message, kind) => { notices.push({ message, kind }); },
    watch: async (setId, roots) => { watched.set(setId, roots); return quiet; },
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
    jest.advanceTimersByTime(REFRESH_TRAILING_MS - 100);
    refresh.changed('/work/b');
    refresh.changed('/work/a');
    jest.advanceTimersByTime(REFRESH_TRAILING_MS);
    expect(refreshed).toEqual([['/work/a', '/work/b']]);
  });

  test('an event after the window starts a new refresh', async () => {
    const { refresh, refreshed } = fixture();
    refresh.changed('/work/a');
    jest.advanceTimersByTime(REFRESH_TRAILING_MS);
    await settle();
    refresh.changed('/work/a');
    jest.advanceTimersByTime(REFRESH_TRAILING_MS);
    expect(refreshed).toEqual([['/work/a'], ['/work/a']]);
  });

  test('a steady stream of events still refreshes once the maximum wait passes', () => {
    const { refresh, refreshed } = fixture();
    for (let elapsed = 0; elapsed < REFRESH_MAX_WAIT_MS; elapsed += 100) {
      refresh.changed('/work/a');
      jest.advanceTimersByTime(100);
    }
    expect(refreshed).toEqual([['/work/a']]);
  });

  test('a repository is never read twice at once and gets one follow-up', async () => {
    let release: () => void = () => {};
    const gate = new Promise<void>(resolve => { release = resolve; });
    const calls: string[][] = [];
    const { refresh } = fixture({ refresh: paths => { calls.push(paths); return calls.length === 1 ? gate : undefined; } });
    refresh.changed('/work/a');
    jest.advanceTimersByTime(REFRESH_TRAILING_MS);
    for (let round = 0; round < 3; round += 1) { refresh.changed('/work/a'); jest.advanceTimersByTime(REFRESH_TRAILING_MS); }
    expect(calls).toEqual([['/work/a']]);
    release();
    await settle();
    expect(calls).toEqual([['/work/a'], ['/work/a']]);
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
    const { refresh } = fixture({ watch: async () => { starts += 1; return quiet; } });
    await refresh.sync(new Map([['a', ['/w/1', '/w/2']]]));
    await refresh.sync(new Map([['a', ['/w/2', '/w/1']]]));
    expect(starts).toBe(1);
  });
});

describe('notices', () => {
  const refused = 'This set has 201 repositories; automatic refresh watches up to 200.';

  test('a refused watch shows one notice per set and keeps the count at zero', async () => {
    const { refresh, notices } = fixture({ watch: async () => { throw refused; } });
    await refresh.sync(new Map([['a', ['/w/1']], ['b', ['/w/2']]]));
    expect(refresh.watching).toBe(0);
    expect(notices.map(entry => entry.message)).toEqual([`${refused} Refresh local status still works.`, `${refused} Refresh local status still works.`]);
  });

  test('a failure notice repeats for a set only after that set started again', async () => {
    let refuse = true;
    const { refresh, notices } = fixture({ watch: async () => { if (refuse) throw 'boom'; return quiet; } });
    await refresh.sync(new Map([['a', ['/w/1']]]));
    await refresh.sync(new Map([['a', ['/w/1', '/w/2']]]));
    expect(notices).toHaveLength(1);
    refuse = false;
    await refresh.sync(new Map([['a', ['/w/3']]]));
    refuse = true;
    await refresh.sync(new Map([['a', ['/w/4']]]));
    expect(notices).toHaveLength(2);
  });

  test('an injected watcher error stops every watch and shows one notice', async () => {
    const { refresh, notices, watched } = fixture();
    await refresh.sync(new Map([['a', ['/w/1']], ['b', ['/w/2']]]));
    await refresh.failed('inotify queue overflowed');
    await refresh.failed('inotify queue overflowed');
    expect(refresh.watching).toBe(0);
    expect(watched.size).toBe(0);
    expect(notices).toEqual([{ message: 'Automatic refresh stopped: inotify queue overflowed. Refresh local status still works.', kind: 'warn' }]);
  });

  test('after a watcher error the next sync restarts every set together', async () => {
    const { refresh, watched } = fixture();
    const targets = new Map([['a', ['/w/1']], ['b', ['/w/2']]]);
    await refresh.sync(targets);
    await refresh.failed('overflow');
    expect(watched.size).toBe(0);
    await refresh.sync(targets);
    expect([...watched.keys()].sort()).toEqual(['a', 'b']);
  });

  test('skipped folders and best-effort locations are one informational note each', async () => {
    const report = { watched: 1, skipped: [{ path: '/w/gone', reason: 'x' }], bestEffort: ['C:\\Users\\a\\OneDrive\\r'] };
    const { refresh, notices } = fixture({ watch: async () => report });
    await refresh.sync(new Map([['a', ['/w/1']]]));
    await refresh.sync(new Map([['a', ['/w/1', '/w/2']]]));
    expect(notices.map(entry => entry.kind)).toEqual(['info', 'info']);
    expect(notices[0].message).toContain('skipped 1 folder: gone');
    expect(notices[1].message).toContain('best effort');
  });

  test('a lost folder is announced once', () => {
    const { refresh, notices } = fixture();
    refresh.lost('/w/gone', 'the folder is gone');
    refresh.lost('/w/gone', 'the folder is gone');
    expect(notices).toHaveLength(1);
    refresh.changed('/w/gone');
    refresh.lost('/w/gone', 'the folder is gone');
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
