import { expect, test } from 'bun:test';
import { missingTreePaths } from './compare-refs';

test('a tree that was invalidated is reported as missing, a loading or failed one is not', () => {
  const trees = { loaded: { data: {} }, loading: { loading: true }, failed: { error: 'x' } };
  expect(missingTreePaths(['loaded', 'loading', 'failed', 'gone', 'gone', ''], trees)).toEqual(['gone']);
  expect(missingTreePaths(['loaded'], trees)).toEqual([]);
  expect(missingTreePaths(['loaded', 'gone'], {})).toEqual(['loaded', 'gone']);
});
