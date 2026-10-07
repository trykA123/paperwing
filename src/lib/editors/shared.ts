import type { EditorEvent, Side } from '../editor';

export const SIDES: readonly Side[] = ['left', 'right'];

export class Listeners {
  private readonly set = new Set<(event: EditorEvent) => void>();
  add(listener: (event: EditorEvent) => void): () => void { this.set.add(listener); return () => { this.set.delete(listener); }; }
  emit(event: EditorEvent) { for (const listener of [...this.set]) listener(event); }
  clear() { this.set.clear(); }
}

/** First step from "no current change" lands on the first change going forward and the last going back. */
export function stepIndex(current: number, direction: 1 | -1, count: number): number {
  if (!count) return -1;
  const from = current < 0 ? (direction > 0 ? -1 : 0) : current;
  return (from + direction + count) % count;
}

export function other(side: Side): Side { return side === 'left' ? 'right' : 'left'; }
