const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import { headerPrimary, type PrimaryFacts } from './header-primary';

const base: PrimaryFacts = { running: false, itemCount: 800, cloneCount: 0, rightVisible: true, fetchable: 700 };

describe('headerPrimary', () => {
  test('nothing selected with uncloned repositories: no footer, so Fetch all is primary', () => {
    expect(headerPrimary({ ...base })).toEqual({ fetch: true, progress: false });
  });

  test('selection with uncloned repositories: the footer keeps the primary', () => {
    expect(headerPrimary({ ...base, cloneCount: 12 })).toEqual({ fetch: false, progress: false });
  });

  test('hidden right panel gives the primary back to the header', () => {
    expect(headerPrimary({ ...base, cloneCount: 12, rightVisible: false })).toEqual({ fetch: true, progress: false });
  });

  test('run active: one dark button only', () => {
    expect(headerPrimary({ ...base, running: true })).toEqual({ fetch: false, progress: true });
    expect(headerPrimary({ ...base, running: true, cloneCount: 5 })).toEqual({ fetch: false, progress: false });
  });

  test('empty set: the header has no primary, the empty state owns it', () => {
    expect(headerPrimary({ ...base, itemCount: 0, fetchable: 0 })).toEqual({ fetch: false, progress: false });
  });
});
