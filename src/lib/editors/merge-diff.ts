import { Change, diff, type DiffConfig } from '@codemirror/merge';
import { diffLines } from './line-diff';

const trimmed = (line: string) => line.trim();

function lineStarts(lines: readonly string[]): Float64Array {
  const starts = new Float64Array(lines.length + 1);
  let at = 0;
  for (let i = 0; i < lines.length; i++) { starts[i] = at; at += lines[i]!.length + 1; }
  starts[lines.length] = at;
  return starts;
}

function ignoringTrimmedWhitespace(a: string, b: string): readonly Change[] {
  const aLines = a.split('\n'), bLines = b.split('\n');
  const aStarts = lineStarts(aLines), bStarts = lineStarts(bLines);
  const changes: Change[] = [];
  for (const [a0, a1, b0, b1] of diffLines(aLines, bLines, trimmed)) {
    const fromA = aStarts[a0]!, toA = Math.min(a.length, aStarts[a1]!), fromB = bStarts[b0]!, toB = Math.min(b.length, bStarts[b1]!);
    for (const change of diff(a.slice(fromA, toA), b.slice(fromB, toB), { scanLimit: 500 })) {
      changes.push(new Change(fromA + change.fromA, fromA + change.toA, fromB + change.fromB, fromB + change.toB));
    }
  }
  return changes;
}

export function diffConfigFor(ignoreWhitespace: boolean): DiffConfig | undefined {
  return ignoreWhitespace ? { override: ignoringTrimmedWhitespace } : undefined;
}
