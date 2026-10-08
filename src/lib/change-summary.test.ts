const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import type { EditorChange } from './editor';
import { countChanges, describeCopy } from './change-summary';

const change = (left: [number, number], right: [number, number]): EditorChange => ({ left: { start: left[0], count: left[1] }, right: { start: right[0], count: right[1] } });

describe('change counts', () => {
  test('an empty left side is an addition, an empty right side a removal, the rest changes', () => {
    expect(countChanges([change([4, 0], [4, 2]), change([9, 3], [9, 0]), change([12, 1], [12, 5]), change([20, 2], [22, 2])])).toEqual({ add: 1, rem: 1, chg: 2 });
  });

  test('no changes count as zero', () => {
    expect(countChanges([])).toEqual({ add: 0, rem: 0, chg: 0 });
  });
});

describe('copy summary', () => {
  test('copying onto an empty span adds the lines after the line before it', () => {
    expect(describeCopy(change([4, 0], [4, 2]), 'left', 0, 3)).toBe('Copy change 1 of 3 to the left file. It adds 2 lines after line 4.');
  });

  test('copying an empty span over lines removes them', () => {
    expect(describeCopy(change([4, 0], [4, 2]), 'right', 1, 3)).toBe('Copy change 2 of 3 to the right file. It removes lines 5–6.');
  });

  test('copying between non-empty spans replaces lines', () => {
    expect(describeCopy(change([10, 1], [10, 3]), 'left', 2, 3)).toBe('Copy change 3 of 3 to the left file. It replaces line 11 with 3 lines from the right file.');
  });
});
