import type { EditorChange, LineSpan, Side } from './editor';
import { plural } from './plural';

export type ChangeCounts = { add: number; rem: number; chg: number };

export function countChanges(changes: readonly EditorChange[]): ChangeCounts {
  const counts: ChangeCounts = { add: 0, rem: 0, chg: 0 };
  for (const change of changes) {
    if (change.left.count === 0) counts.add++;
    else if (change.right.count === 0) counts.rem++;
    else counts.chg++;
  }
  return counts;
}

const lines = (count: number) => plural(count, 'line', 'lines');

function range(span: LineSpan): string {
  return span.count === 1 ? `line ${span.start + 1}` : `lines ${span.start + 1}–${span.start + span.count}`;
}

/** What copying one change over to the other side will do, for the confirm bar. */
export function describeCopy(change: EditorChange, to: Side, index: number, total: number): string {
  const from: Side = to === 'left' ? 'right' : 'left';
  const target = change[to], source = change[from];
  const head = `Copy change ${index + 1} of ${total} to the ${to} file.`;
  if (target.count === 0) return `${head} It adds ${lines(source.count)} after line ${target.start}.`;
  if (source.count === 0) return `${head} It removes ${range(target)}.`;
  return `${head} It replaces ${range(target)} with ${lines(source.count)} from the ${from} file.`;
}
