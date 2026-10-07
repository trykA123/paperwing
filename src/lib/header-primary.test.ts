const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import { headerPrimary, type PrimaryFacts } from './header-primary';

const base: PrimaryFacts = { running: false, itemCount: 800, cloneCount: 0, fetchable: 700 };

describe('headerPrimary', () => {
  test('everything cloned: Fetch is primary', () => {
    expect(headerPrimary({ ...base })).toEqual({ clone: false, fetch: true, progress: false });
  });

  test('repositories missing from disk: Clone is primary and Fetch is not', () => {
    expect(headerPrimary({ ...base, cloneCount: 12 })).toEqual({ clone: true, fetch: false, progress: false });
  });

  test('nothing cloned and nothing missing to fetch: no primary', () => {
    expect(headerPrimary({ ...base, fetchable: 0 })).toEqual({ clone: false, fetch: false, progress: false });
  });

  test('run active: one dark button only', () => {
    expect(headerPrimary({ ...base, running: true })).toEqual({ clone: false, fetch: false, progress: true });
    expect(headerPrimary({ ...base, running: true, cloneCount: 5 })).toEqual({ clone: false, fetch: false, progress: true });
  });

  test('empty set: the bar has no primary, the empty state owns it', () => {
    expect(headerPrimary({ ...base, itemCount: 0, fetchable: 0 })).toEqual({ clone: false, fetch: false, progress: false });
  });
});
