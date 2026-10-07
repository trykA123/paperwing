const testModule = 'bun:test';
const { expect, test } = await import(testModule);
import { Text } from '@codemirror/state';
import { linesOf, replaceSpan, spanOf } from './line-spans';

const doc = (text: string) => Text.of(text.split('\n'));
const apply = (text: string, start: number, count: number, incoming: string[]) => {
  const edit = replaceSpan(doc(text), { start, count }, incoming);
  return text.slice(0, edit.from) + edit.insert + text.slice(edit.to);
};

test('replacing lines swaps exactly those lines', () => {
  expect(apply('one\nold\nthree', 1, 1, ['changed'])).toBe('one\nchanged\nthree');
  expect(apply('a\nb\nc', 0, 3, ['x', 'y'])).toBe('x\ny');
  expect(apply('a\nb\nc\n', 1, 1, ['B', 'B2'])).toBe('a\nB\nB2\nc\n');
});

test('inserting lines goes before the given line or after the last one', () => {
  expect(apply('one\nthree', 1, 0, ['inserted'])).toBe('one\ninserted\nthree');
  expect(apply('one\ntwo\n', 2, 0, ['three'])).toBe('one\ntwo\nthree\n');
  expect(apply('one\ntwo', 2, 0, ['three'])).toBe('one\ntwo\nthree');
  expect(apply('', 0, 0, ['only'])).toBe('only\n');
});

test('deleting lines removes their line breaks too', () => {
  expect(apply('one\nremoved\nthree', 1, 1, [])).toBe('one\nthree');
  expect(apply('one\ntwo\nextra', 2, 1, [])).toBe('one\ntwo');
  expect(apply('one\ntwo', 0, 1, [])).toBe('two');
  expect(apply('only', 0, 1, [])).toBe('');
  expect(apply('a\nb', 0, 0, [])).toBe('a\nb');
});

test('chunk ranges become whole-line spans', () => {
  const text = doc('a\nb\nc\n');
  expect(spanOf(text, 2, 4)).toEqual({ start: 1, count: 1 });
  expect(spanOf(text, 2, 6)).toEqual({ start: 1, count: 2 });
  expect(spanOf(text, 2, 2)).toEqual({ start: 1, count: 0 });
  expect(spanOf(text, 6, 6)).toEqual({ start: 3, count: 0 });
  expect(linesOf(text, { start: 1, count: 2 })).toEqual(['b', 'c']);
  expect(linesOf(text, { start: 1, count: 0 })).toEqual([]);
});
