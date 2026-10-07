const testModule = 'bun:test';
const { expect, test } = await import(testModule);
import { diffLines, type LineHunk } from './line-diff';

function random(seed: number) {
  let state = seed >>> 0;
  return () => { state = (Math.imul(state, 1664525) + 1013904223) >>> 0; return state / 4294967296; };
}

const apply = (a: string[], b: string[], hunks: readonly LineHunk[]) => {
  const out: string[] = [];
  let at = 0;
  for (const [a0, a1, b0, b1] of hunks) { out.push(...a.slice(at, a0), ...b.slice(b0, b1)); at = a1; }
  return [...out, ...a.slice(at)];
};

const lcs = (a: string[], b: string[]) => {
  const table = Array.from({ length: a.length + 1 }, () => new Array(b.length + 1).fill(0));
  for (let i = 1; i <= a.length; i++) for (let j = 1; j <= b.length; j++) table[i][j] = a[i - 1] === b[j - 1] ? table[i - 1][j - 1] + 1 : Math.max(table[i - 1][j], table[i][j - 1]);
  return table[a.length][b.length];
};

test('hunks rebuild the second file and are minimal for repetitive lines', () => {
  const next = random(7);
  for (let round = 0; round < 60; round++) {
    const make = () => Array.from({ length: Math.floor(next() * 40) }, () => 'abc'[Math.floor(next() * 3)]!);
    const a = make(), b = make(), hunks = diffLines(a, b);
    expect(apply(a, b, hunks)).toEqual(b);
    const edits = hunks.reduce((sum, [a0, a1, b0, b1]) => sum + (a1 - a0) + (b1 - b0), 0);
    expect(edits).toBe(a.length + b.length - 2 * lcs(a, b));
  }
});

test('a gap that needs more steps than the limit becomes one coarse hunk', () => {
  const a = ['a', 'b', 'a', 'b', 'a', 'b'], b = ['b', 'a', 'b', 'a', 'b', 'a'];
  expect(diffLines(a, b, undefined, 1)).toEqual([[0, 6, 0, 6]]);
  expect(apply(a, b, diffLines(a, b, undefined, 1))).toEqual(b);
});

test('a large repetitive gap stays within the default step limit and still finishes', () => {
  const a = Array.from({ length: 6000 }, (_, i) => (i % 2 ? 'x' : 'y')), b = Array.from({ length: 6000 }, (_, i) => (i % 3 ? 'x' : 'z'));
  const hunks = diffLines(a, b);
  expect(apply(a, b, hunks)).toEqual(b);
});
