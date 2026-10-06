import { expect, test } from 'bun:test';
import { countLabel, errorSummary, newFailures } from './source-status.ts';

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
