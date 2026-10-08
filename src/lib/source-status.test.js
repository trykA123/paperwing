import { expect, test } from 'bun:test';
import { countLabel, errorSummary, isDisabled, isRepoDisabled, newFailures } from './source-status.ts';

test('unknown counts render a dash, never zero', () => {
  expect(countLabel(false, 0)).toBe('–');
  expect(countLabel(true, 0)).toBe('0');
  expect(countLabel(true, 12)).toBe('12');
});

test('error text is deduplicated and joined', () => {
  expect(errorSummary(undefined)).toBe('');
  expect(errorSummary(['HTTP 401', 'HTTP 401', 'timeout'])).toBe('HTTP 401 · timeout');
});

test('one notice per failing source, again only when its error changes or clears', () => {
  const raised = new Map();
  const ids = ['a', 'b'];
  expect(newFailures(raised, { a: ['401'], b: [] }, ids)).toEqual([{ id: 'a', message: '401' }]);
  expect(newFailures(raised, { a: ['401'], b: [] }, ids)).toEqual([]);
  expect(newFailures(raised, { a: ['500'], b: ['401'] }, ids)).toEqual([{ id: 'a', message: '500' }, { id: 'b', message: '401' }]);
  expect(newFailures(raised, { a: [], b: ['401'] }, ids)).toEqual([]);
  expect(newFailures(raised, { a: ['500'], b: ['401'] }, ids)).toEqual([{ id: 'a', message: '500' }]);
});

test('a source is disabled only when it says so, and its repositories follow it', () => {
  const sources = [{ id: 'gh', enabled: false }, { id: 'ghe' }, { id: 'jira', enabled: true }];
  expect(isDisabled(sources[0])).toBe(true);
  expect(isDisabled(sources[1])).toBe(false);
  expect(isDisabled(undefined)).toBe(false);
  expect(isRepoDisabled('gh:acme/api', sources)).toBe(true);
  expect(isRepoDisabled('ghe:acme/api', sources)).toBe(false);
  expect(isRepoDisabled('unknown:acme/api', sources)).toBe(false);
});
