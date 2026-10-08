const testModule = 'bun:test';
const { expect, test } = await import(testModule);
import { Text } from '@codemirror/state';
import type { BandChunk } from './chunk-bands';
import { markedLines } from './mark-lines';

const doc = Text.of(Array.from({ length: 12 }, (_, index) => `line ${index + 1}`));
const at = (line: number) => doc.line(line).from;
const chunk = (a: [number, number], b: [number, number]): BandChunk => ({ fromA: a[0], toA: a[1], endA: Math.max(a[0], a[1] - 1), fromB: b[0], toB: b[1], endB: Math.max(b[0], b[1] - 1) });
const all = { from: 0, to: doc.length };

test('every line of a changed range is marked, with the first and last flagged', () => {
  const lines = markedLines([chunk([0, 0], [at(3), at(6)])], doc, 'b', all);
  expect(lines.map(line => [line.from, line.first, line.last, line.kind])).toEqual([[at(3), true, false, 'add'], [at(4), false, false, 'add'], [at(5), false, true, 'add']]);
});

test('the side with no lines in a change gets no marks', () => {
  expect(markedLines([chunk([0, 0], [at(3), at(6)])], doc, 'a', all)).toEqual([]);
});

test('a change that ends at the end of the document marks its last line', () => {
  const end = doc.length;
  const lines = markedLines([chunk([at(11), end + 1], [at(11), end + 1])], doc, 'a', all);
  expect(lines.map(line => line.from)).toEqual([at(11), at(12)]);
});

test('changes outside the viewport are skipped', () => {
  const chunks = [chunk([at(1), at(2)], [at(1), at(2)]), chunk([at(10), at(11)], [at(10), at(11)])];
  expect(markedLines(chunks, doc, 'b', { from: at(5), to: at(8) })).toEqual([]);
  expect(markedLines(chunks, doc, 'b', { from: at(9), to: doc.length }).map(line => line.from)).toEqual([at(10)]);
});
