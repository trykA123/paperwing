export type RangeResult = { on: string[]; off: string[]; added: Set<string> };

/**
 * Extends or shrinks a Shift range from a fixed anchor. Rows the range switched on are tracked,
 * so moving back toward the anchor switches only those off again.
 */
export function shiftRange(ids: readonly string[], anchor: string, target: string, isOn: (id: string) => boolean, added: ReadonlySet<string>): RangeResult {
  const a = ids.indexOf(anchor);
  const b = ids.indexOf(target);
  if (a < 0 || b < 0) return { on: [], off: [], added: new Set(added) };
  const inside = new Set(ids.slice(Math.min(a, b), Math.max(a, b) + 1));
  const next = new Set<string>();
  const on: string[] = [];
  const off: string[] = [];
  for (const id of added) { if (inside.has(id)) next.add(id); else off.push(id); }
  for (const id of inside) if (!isOn(id)) { on.push(id); next.add(id); }
  return { on, off, added: next };
}

/** Shift+click that clears: every row between anchor and target is switched off. */
export function clearRange(ids: readonly string[], anchor: string, target: string): string[] {
  const a = ids.indexOf(anchor);
  const b = ids.indexOf(target);
  return a < 0 || b < 0 ? [target] : ids.slice(Math.min(a, b), Math.max(a, b) + 1);
}

/** The "Select all N" offer: the whole page is on and more rows are shown than the page holds. */
export function selectionOffer<T extends { on: boolean }>(page: readonly T[], shown: readonly T[]): number {
  if (!page.length || shown.length <= page.length) return 0;
  return page.every(item => item.on) && !shown.every(item => item.on) ? shown.length : 0;
}
