const testModule = 'bun:test';
const { expect, test } = await import(testModule);
import { diffLines } from './line-diff';
import { buildRows, GAP_MARGIN, ROW_ADDED, ROW_CHANGED, ROW_EQUAL, ROW_GAP, ROW_REMOVED } from './viewer-rows';

const rowsOf = (a: string[], b: string[], layout: 'sideBySide' | 'inline', hideUnchanged = false) =>
  buildRows({ leftLines: a.length, rightLines: b.length, hunks: diffLines(a, b), layout, hideUnchanged });
const kinds = (rows: ReturnType<typeof buildRows>) => Array.from(rows.kind.subarray(0, rows.count));

test('side by side pairs changed lines and leaves a void for the shorter side', () => {
  const rows = rowsOf(['a', 'b', 'c'], ['a', 'B', 'B2', 'c'], 'sideBySide');
  expect(kinds(rows)).toEqual([ROW_EQUAL, ROW_CHANGED, ROW_ADDED, ROW_EQUAL]);
  expect(Array.from(rows.left)).toEqual([0, 1, -1, 2]);
  expect(Array.from(rows.right)).toEqual([0, 1, 2, 3]);
  expect(Array.from(rows.changeRows)).toEqual([1]);
});

test('inline lists removed lines before added lines', () => {
  const rows = rowsOf(['a', 'b', 'c'], ['a', 'B', 'c'], 'inline');
  expect(kinds(rows)).toEqual([ROW_EQUAL, ROW_REMOVED, ROW_ADDED, ROW_EQUAL]);
  expect(Array.from(rows.changeRows)).toEqual([1]);
});

test('hiding unchanged lines keeps a margin around each change and collapses the rest', () => {
  const left = Array.from({ length: 40 }, (_, i) => `line ${i}`), right = left.map((line, i) => (i === 20 ? 'changed' : line));
  const rows = rowsOf(left, right, 'sideBySide', true);
  expect(rows.count).toBe(GAP_MARGIN + 1 + 1 + GAP_MARGIN + 1);
  expect(kinds(rows)).toEqual([ROW_GAP, ...Array(GAP_MARGIN).fill(ROW_EQUAL), ROW_CHANGED, ...Array(GAP_MARGIN).fill(ROW_EQUAL), ROW_GAP]);
  expect([rows.left[0], rows.left[rows.count - 1]]).toEqual([17, 16]);
  expect(Array.from(rows.changeRows)).toEqual([GAP_MARGIN + 1]);
});

test('identical files produce no change rows', () => {
  const rows = rowsOf(['a', 'b'], ['a', 'b'], 'sideBySide');
  expect(rows.changeRows.length).toBe(0);
  expect(rows.count).toBe(2);
});
