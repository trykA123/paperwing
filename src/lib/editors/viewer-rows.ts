import type { EditorLayout } from '../editor';
import type { LineHunk } from './line-diff';

export const ROW_EQUAL = 0, ROW_REMOVED = 1, ROW_ADDED = 2, ROW_CHANGED = 3, ROW_GAP = 4;
export const GAP_MARGIN = 3, GAP_MIN = 4;

/** For a gap row `left` holds the number of collapsed lines; elsewhere `left`/`right` are line indexes, or -1 where that side has no line. */
export type Rows = { kind: Uint8Array; left: Int32Array; right: Int32Array; count: number; changeRows: Int32Array };
type RowsInput = { leftLines: number; rightLines: number; hunks: readonly LineHunk[]; layout: EditorLayout; hideUnchanged: boolean };

function rowCount({ leftLines, hunks, layout }: RowsInput): number {
  let total = leftLines;
  for (const [a0, a1, b0, b1] of hunks) total += layout === 'inline' ? b1 - b0 : Math.max(a1 - a0, b1 - b0) - (a1 - a0);
  return total;
}

function expand(input: RowsInput): Rows {
  const count = rowCount(input);
  const kind = new Uint8Array(count), left = new Int32Array(count), right = new Int32Array(count);
  let row = 0, a = 0, b = 0;
  const equalUntil = (limit: number) => { for (; a < limit; a++, b++, row++) { kind[row] = ROW_EQUAL; left[row] = a; right[row] = b; } };
  for (const [a0, a1, b0, b1] of input.hunks) {
    equalUntil(a0);
    const removed = a1 - a0, added = b1 - b0;
    if (input.layout === 'inline') {
      for (let i = 0; i < removed; i++, row++) { kind[row] = ROW_REMOVED; left[row] = a0 + i; right[row] = -1; }
      for (let i = 0; i < added; i++, row++) { kind[row] = ROW_ADDED; left[row] = -1; right[row] = b0 + i; }
    } else {
      for (let i = 0; i < Math.max(removed, added); i++, row++) {
        kind[row] = i < removed && i < added ? ROW_CHANGED : i < removed ? ROW_REMOVED : ROW_ADDED;
        left[row] = i < removed ? a0 + i : -1; right[row] = i < added ? b0 + i : -1;
      }
    }
    a = a1; b = b1;
  }
  equalUntil(input.leftLines);
  return { kind, left, right, count, changeRows: new Int32Array(0) };
}

function collapse(rows: Rows): Rows {
  const kind: number[] = [], left: number[] = [], right: number[] = [];
  const keep = (row: number) => { kind.push(rows.kind[row]!); left.push(rows.left[row]!); right.push(rows.right[row]!); };
  for (let start = 0; start < rows.count;) {
    let end = start;
    const equal = rows.kind[start] === ROW_EQUAL;
    while (end < rows.count && (rows.kind[end] === ROW_EQUAL) === equal) end++;
    const before = start > 0 ? GAP_MARGIN : 0, after = end < rows.count ? GAP_MARGIN : 0;
    const hidden = end - start - before - after;
    if (!equal || hidden < GAP_MIN) { for (let row = start; row < end; row++) keep(row); }
    else {
      for (let row = start; row < start + before; row++) keep(row);
      kind.push(ROW_GAP); left.push(hidden); right.push(-1);
      for (let row = end - after; row < end; row++) keep(row);
    }
    start = end;
  }
  return { kind: Uint8Array.from(kind), left: Int32Array.from(left), right: Int32Array.from(right), count: kind.length, changeRows: rows.changeRows };
}

function withChangeRows(rows: Rows): Rows {
  const starts: number[] = [];
  for (let row = 0; row < rows.count; row++) {
    const changed = rows.kind[row] !== ROW_EQUAL && rows.kind[row] !== ROW_GAP;
    const after = row === 0 || rows.kind[row - 1] === ROW_EQUAL || rows.kind[row - 1] === ROW_GAP;
    if (changed && after) starts.push(row);
  }
  return { ...rows, changeRows: Int32Array.from(starts) };
}

export function buildRows(input: RowsInput): Rows {
  const rows = expand(input);
  return withChangeRows(input.hideUnchanged ? collapse(rows) : rows);
}
