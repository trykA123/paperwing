import type { Text } from '@codemirror/state';
import type { LineSpan } from '../editor';

export type TextEdit = { from: number; to: number; insert: string };

/** Converts a chunk range (offsets, `to` past the last line break) into whole lines. */
export function spanOf(doc: Text, from: number, to: number): LineSpan {
  const first = doc.lineAt(from);
  if (from === to) return { start: from === first.from ? first.number - 1 : first.number, count: 0 };
  return { start: first.number - 1, count: doc.lineAt(Math.max(from, to - 1)).number - first.number + 1 };
}

export function linesOf(doc: Text, span: LineSpan): string[] {
  if (span.count === 0) return [];
  return doc.sliceString(doc.line(span.start + 1).from, doc.line(span.start + span.count).to).split('\n');
}

/** The edit that replaces the lines of `span` in `doc` with `incoming`. Lines are joined by `\n`, so the document keeps its final-newline state. */
export function replaceSpan(doc: Text, span: LineSpan, incoming: readonly string[]): TextEdit {
  const { start, count } = span;
  if (incoming.length && count) return { from: doc.line(start + 1).from, to: doc.line(start + count).to, insert: incoming.join('\n') };
  if (incoming.length) {
    if (start < doc.lines) return { from: doc.line(start + 1).from, to: doc.line(start + 1).from, insert: incoming.join('\n') + '\n' };
    return { from: doc.length, to: doc.length, insert: '\n' + incoming.join('\n') };
  }
  if (!count) return { from: 0, to: 0, insert: '' };
  if (start + count < doc.lines) return { from: doc.line(start + 1).from, to: doc.line(start + count + 1).from, insert: '' };
  return { from: start > 0 ? doc.line(start).to : 0, to: doc.length, insert: '' };
}
