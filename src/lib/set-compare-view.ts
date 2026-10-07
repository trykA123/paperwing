import type { SetCompareRow } from './set-compare.svelte';

export type ResultKind = 'different' | 'leftOnly' | 'rightOnly' | 'same' | 'partial' | 'pending';
export type SortKey = 'folder' | 'result' | 'different' | 'leftOnly' | 'rightOnly' | 'lines' | 'history';
export type Sort = { key: SortKey; dir: 'asc' | 'desc' };

export const RESULT_GLYPH: Record<ResultKind, string> = { different: '!=', leftOnly: '−', rightOnly: '+', same: '=', partial: '?', pending: '' };
const RESULT_LABEL: Record<ResultKind, string> = { different: 'Different', leftOnly: 'Only left', rightOnly: 'Only right', same: 'Identical', partial: 'Partly unavailable', pending: '' };
const RESULT_ORDER: ResultKind[] = ['different', 'partial', 'leftOnly', 'rightOnly', 'same', 'pending'];

export type RowView = { kind: ResultKind; label: string; different: number | null; leftOnly: number | null; rightOnly: number | null; changed: number };

export function rowView(row: SetCompareRow): RowView {
  const display = row.snapshot?.display;
  if (!display) return { kind: 'pending', label: row.state, different: null, leftOnly: null, rightOnly: null, changed: 0 };
  const different = display.different + display.typeConflict;
  const changed = different + display.leftOnly + display.rightOnly;
  const kind: ResultKind = display.unavailable ? 'partial'
    : different || (display.leftOnly && display.rightOnly) ? 'different'
    : display.leftOnly ? 'leftOnly' : display.rightOnly ? 'rightOnly' : 'same';
  return { kind, label: RESULT_LABEL[kind], different, leftOnly: display.leftOnly, rightOnly: display.rightOnly, changed };
}

export const hasDifferences = (row: SetCompareRow) => {
  const display = row.snapshot?.display;
  return !display || display.different + display.leftOnly + display.rightOnly + display.typeConflict + display.unavailable > 0;
};

function sortValue(row: SetCompareRow, key: SortKey): string | number | null {
  const view = rowView(row);
  switch (key) {
    case 'folder': return row.folder.toLowerCase();
    case 'result': return RESULT_ORDER.indexOf(view.kind);
    case 'different': return view.different;
    case 'leftOnly': return view.leftOnly;
    case 'rightOnly': return view.rightOnly;
    case 'lines': return row.added === null ? null : row.added + (row.removed ?? 0);
    case 'history': return row.snapshot?.history.available ? row.snapshot.history.rightCount : null;
  }
}

export function sortRows(rows: SetCompareRow[], sort: Sort | null): SetCompareRow[] {
  if (!sort) return rows;
  const sign = sort.dir === 'asc' ? 1 : -1;
  return [...rows].sort((a, b) => {
    const left = sortValue(a, sort.key), right = sortValue(b, sort.key);
    if (left === null || right === null) return left === right ? 0 : left === null ? 1 : -1;
    return (left < right ? -1 : left > right ? 1 : 0) * sign;
  });
}

export const nextSort = (current: Sort | null, key: SortKey): Sort | null =>
  current?.key !== key ? { key, dir: 'asc' } : current.dir === 'asc' ? { key, dir: 'desc' } : null;
