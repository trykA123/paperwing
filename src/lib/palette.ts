import { fuzzy } from './fuzzy';

export type Entry<T> = { command: T; group: string; positions: number[] };
type Shape = { label: string; enabled: boolean };

type Scoring<T> = { text: (command: T) => string; bonus?: (command: T, query: string) => number };

/** Empty query keeps the group order; a query ranks commands and repository hits together in one scored list. */
export function arrange<T extends Shape>(list: T[], query: string, groups: readonly string[], groupOf: (command: T) => string, scoring?: Scoring<T>): Entry<T>[] {
  const q = query.trim();
  if (q) {
    const text = scoring?.text ?? ((command: T) => command.label);
    const hits = list.flatMap((command, index) => {
      const found = fuzzy(q, text(command));
      return found ? [{ command, index, positions: found.positions, score: found.score + (scoring?.bonus?.(command, q) ?? 0) }] : [];
    });
    return hits.sort((a, b) => b.score - a.score || a.index - b.index).map(({ command, positions }) => ({ command, group: groupOf(command), positions }));
  }
  const entries = list.map((command, index) => ({ command, index, group: groupOf(command), positions: [] as number[] }));
  const order = (group: string) => { const at = groups.indexOf(group); return at < 0 ? groups.length : at; };
  return entries.sort((a, b) => order(a.group) - order(b.group) || a.index - b.index).map(({ command, group, positions }) => ({ command, group, positions }));
}

/** Bonus for a repository whose name equals or starts with the query. */
export function nameBonus(name: string, query: string): number {
  const n = name.toLowerCase();
  const q = query.trim().toLowerCase();
  return n === q ? 200 : n.startsWith(q) ? 60 : 0;
}

/** The first enabled entry that does not push, close, delete or overwrite; -1 when only risky ones remain. */
export function defaultIndex<T extends Shape>(entries: Entry<T>[], risky: (command: T) => boolean): number {
  return entries.findIndex(entry => entry.command.enabled && !risky(entry.command));
}

/** Arrow keys skip disabled entries and wrap around. */
export function stepIndex<T extends Shape>(entries: Entry<T>[], from: number, direction: 1 | -1): number {
  const length = entries.length;
  for (let n = 1; n <= length; n++) {
    const at = (from + direction * n + length * n) % length;
    if (entries[at].command.enabled) return at;
  }
  return from;
}

const RISKY = new Set(['push', 'tab-close', 'cleanup', 'copy-left', 'copy-right', 'file-undo', 'hunk-left', 'hunk-right']);

/** Push, close, delete and overwrite commands are never preselected. */
export const isRisky = (command: { id: string }): boolean => RISKY.has(command.id);
