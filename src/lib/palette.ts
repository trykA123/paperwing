import { rank } from './fuzzy';

export type Entry<T> = { command: T; group: string; positions: number[] };
type Shape = { label: string; enabled: boolean };

/** Empty query keeps the group order; a query ranks by match quality. */
export function arrange<T extends Shape>(list: T[], query: string, groups: readonly string[], groupOf: (command: T) => string): Entry<T>[] {
  if (query.trim()) return rank(query, list, command => command.label).map(({ item, positions }) => ({ command: item, group: groupOf(item), positions }));
  const entries = list.map((command, index) => ({ command, index, group: groupOf(command), positions: [] as number[] }));
  const order = (group: string) => { const at = groups.indexOf(group); return at < 0 ? groups.length : at; };
  return entries.sort((a, b) => order(a.group) - order(b.group) || a.index - b.index).map(({ command, group, positions }) => ({ command, group, positions }));
}

/** The first enabled entry that does not close or destroy anything. */
export function defaultIndex<T extends Shape>(entries: Entry<T>[], risky: (command: T) => boolean): number {
  const safe = entries.findIndex(entry => entry.command.enabled && !risky(entry.command));
  return safe >= 0 ? safe : Math.max(0, entries.findIndex(entry => entry.command.enabled));
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
