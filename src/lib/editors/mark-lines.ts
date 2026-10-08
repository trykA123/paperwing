import type { Text } from '@codemirror/state';
import { kindOf, type BandChunk, type ChangeKind } from './chunk-bands';

export type MarkedLine = { from: number; kind: ChangeKind; first: boolean; last: boolean };

/** The lines of this side that belong to a change, for the chunks that touch the viewport. */
export function markedLines(chunks: readonly BandChunk[], doc: Text, side: 'a' | 'b', viewport: { from: number; to: number }): MarkedLine[] {
  const lines: MarkedLine[] = [];
  for (const chunk of chunks) {
    const [from, to, end] = side === 'a' ? [chunk.fromA, chunk.toA, chunk.endA] : [chunk.fromB, chunk.toB, chunk.endB];
    if (from >= viewport.to && from !== to) break;
    if (from === to || end < viewport.from) continue;
    const first = doc.lineAt(from).number, last = doc.lineAt(Math.min(end, doc.length)).number;
    for (let number = first; number <= last; number++) {
      lines.push({ from: doc.line(number).from, kind: kindOf(chunk), first: number === first, last: number === last });
    }
  }
  return lines;
}
