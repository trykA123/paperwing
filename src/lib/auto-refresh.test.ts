const testModule = 'bun:test';
const { afterEach, beforeEach, describe, expect, jest, test } = await import(testModule);
import { AutoRefresh, REFRESH_WINDOW_MS, type AutoRefreshHost } from './auto-refresh';

function fixture(overrides: Partial<AutoRefreshHost> = {}) {
  const refreshed: string[][] = [];
  const host: AutoRefreshHost = {
    refresh: paths => { refreshed.push(paths); },
    ...overrides,
  };
  return { refresh: new AutoRefresh(host), refreshed };
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
