import type { Activity, ActivityDelta } from '../api';

export const ACTIVITY_LIMIT = 64;

export type DeltaResult = { entries: ReadonlyMap<string, Activity>; resync: boolean };

export function applyDelta(entries: ReadonlyMap<string, Activity>, delta: ActivityDelta): DeltaResult {
  const { from, lines, ...meta } = delta;
  const current = entries.get(delta.id);
  const known = current?.output.length ?? 0;
  if (from > known) return { entries, resync: true };
  if (current && delta.sequence < current.sequence) return { entries, resync: false };
  const fresh = lines.slice(known - from);
  const output = current ? [...current.output, ...fresh] : fresh;
  return { entries: new Map(entries).set(delta.id, { ...meta, output }), resync: false };
}

export function applyEntry(entries: ReadonlyMap<string, Activity>, entry: Activity): ReadonlyMap<string, Activity> {
  const current = entries.get(entry.id);
  if (current && current.sequence >= entry.sequence) return entries;
  return new Map(entries).set(entry.id, entry);
}

export function sortedActivity(entries: ReadonlyMap<string, Activity>): Activity[] {
  return [...entries.values()].sort((left, right) => left.startedAt - right.startedAt);
}

export function trimActivity(entries: ReadonlyMap<string, Activity>): ReadonlyMap<string, Activity> {
  if (entries.size <= ACTIVITY_LIMIT) return entries;
  const trimmed = new Map(entries);
  for (const entry of sortedActivity(trimmed)) {
    if (trimmed.size <= ACTIVITY_LIMIT) break;
    if (entry.state !== 'running') trimmed.delete(entry.id);
  }
  return trimmed;
}
