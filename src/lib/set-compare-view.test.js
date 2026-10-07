import { expect, test } from 'bun:test';
import { hasDifferences, nextSort, rowView, sortRows } from './set-compare-view.ts';

const display = (over = {}) => ({ same: 0, different: 0, leftOnly: 0, rightOnly: 0, typeConflict: 0, unavailable: 0, total: 0, ...over });
const row = (folder, over = {}, added = null) => ({ itemId: folder, folder, name: folder, state: 'ready', added, removed: added === null ? null : 0,
  snapshot: over === null ? null : { display: display(over), history: { available: false } } });

test('row results map to a kind with a glyph-able class', () => {
  expect(rowView(row('a', { same: 3 })).kind).toBe('same');
  expect(rowView(row('a', { different: 1 })).kind).toBe('different');
  expect(rowView(row('a', { leftOnly: 2 })).kind).toBe('leftOnly');
  expect(rowView(row('a', { rightOnly: 2 })).kind).toBe('rightOnly');
  expect(rowView(row('a', { leftOnly: 1, rightOnly: 1 })).kind).toBe('different');
  expect(rowView(row('a', { different: 1, unavailable: 1 })).kind).toBe('partial');
  expect(rowView(row('a', null))).toMatchObject({ kind: 'pending', label: 'ready', different: null });
});

test('differences-only keeps pending rows and rows with changes', () => {
  expect(hasDifferences(row('a', { same: 4 }))).toBe(false);
  expect(hasDifferences(row('a', { rightOnly: 1 }))).toBe(true);
  expect(hasDifferences(row('a', null))).toBe(true);
});

test('sorting is stable per column, puts unknown values last and cycles asc, desc, off', () => {
  const rows = [row('b', { different: 5 }), row('a', { different: 9 }), row('c', null)];
  expect(sortRows(rows, { key: 'different', dir: 'asc' }).map(r => r.folder)).toEqual(['b', 'a', 'c']);
  expect(sortRows(rows, { key: 'different', dir: 'desc' }).map(r => r.folder)).toEqual(['a', 'b', 'c']);
  expect(sortRows(rows, { key: 'folder', dir: 'asc' }).map(r => r.folder)).toEqual(['a', 'b', 'c']);
  expect(sortRows(rows, null)).toBe(rows);
  const asc = nextSort(null, 'folder');
  expect(asc).toEqual({ key: 'folder', dir: 'asc' });
  expect(nextSort(asc, 'folder')).toEqual({ key: 'folder', dir: 'desc' });
  expect(nextSort({ key: 'folder', dir: 'desc' }, 'folder')).toBeNull();
  expect(nextSort(asc, 'result')).toEqual({ key: 'result', dir: 'asc' });
});
