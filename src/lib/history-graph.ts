import type { HistoryCommit, RepositoryHistory, TagInfo } from './api';

export const ROW_HEIGHT = 32;
export const RAIL_X = { local: 14, origin: 34 } as const;
export const GRAPH_WIDTH = 48;

export type RailName = keyof typeof RAIL_X;
export type RowKind = 'uncommitted' | 'local' | 'origin' | 'base' | 'below' | 'more';
export type GraphRow = {
  id: string; kind: RowKind; rail: RailName; x: number; y: number; commit: HistoryCommit | null; label: string; tag: string | null; tags: string[]; count: number;
};
export type RailPath = { id: string; rail: RailName; d: string };
export type GraphLayout = { rows: GraphRow[]; paths: RailPath[]; height: number; width: number };

const plural = (count: number, word: string) => `${count} ${word}${count === 1 ? '' : 's'}`;
const centre = (index: number) => index * ROW_HEIGHT + ROW_HEIGHT / 2;

function commitRows(kind: RowKind, rail: RailName, commits: readonly HistoryCommit[], firstTag: string | null, byCommit: ReadonlyMap<string, string[]>): Omit<GraphRow, 'y'>[] {
  return commits.map((commit, index) => ({
    id: `${kind}:${commit.sha}`, kind, rail, x: RAIL_X[rail], commit, label: commit.subject, tag: index === 0 ? firstTag : null, tags: byCommit.get(commit.sha) ?? [], count: 0,
  }));
}

function moreRow(rail: RailName, hidden: number, noun: string): Omit<GraphRow, 'y'>[] {
  if (hidden <= 0) return [];
  return [{ id: `more:${rail}`, kind: 'more', rail, x: RAIL_X[rail], commit: null, label: `${plural(hidden, noun)} not shown`, tag: null, tags: [], count: hidden }];
}

/** Tag names by the full id of the commit they point at. */
export function tagsByCommit(tags: readonly TagInfo[]): Map<string, string[]> {
  const map = new Map<string, string[]>();
  for (const tag of tags) map.set(tag.commit, [...(map.get(tag.commit) ?? []), tag.name]);
  return map;
}

function buildRows(history: RepositoryHistory, byCommit: ReadonlyMap<string, string[]>): GraphRow[] {
  const localTag = history.kind === 'tracking' ? 'local' : 'HEAD';
  const loose: Omit<GraphRow, 'y'>[] = [];
  if (history.uncommitted > 0) {
    loose.push({ id: 'uncommitted', kind: 'uncommitted', rail: 'local', x: RAIL_X.local, commit: null, label: plural(history.uncommitted, 'uncommitted file'), tag: 'working tree', tags: [], count: history.uncommitted });
  }
  loose.push(...commitRows('local', 'local', history.local, localTag, byCommit), ...moreRow('local', history.localTotal - history.local.length, 'local commit'));
  loose.push(...commitRows('origin', 'origin', history.origin, 'origin', byCommit), ...moreRow('origin', history.originTotal - history.origin.length, 'origin commit'));
  if (history.base) loose.push(...commitRows('base', 'local', [history.base], 'shared base', byCommit));
  loose.push(...commitRows('below', 'local', history.below, null, byCommit));
  return loose.map((row, index) => ({ ...row, y: centre(index) }));
}

function railPaths(rows: readonly GraphRow[]): RailPath[] {
  const paths: RailPath[] = [];
  const onLocal = rows.filter(row => row.rail === 'local');
  const onOrigin = rows.filter(row => row.rail === 'origin');
  const base = rows.find(row => row.kind === 'base');
  const first = onLocal[0];
  const last = onLocal.at(-1);
  if (first && last && first.y !== last.y) paths.push({ id: 'local', rail: 'local', d: `M${RAIL_X.local} ${first.y} V${last.y}` });
  const top = onOrigin[0];
  const bottom = onOrigin.at(-1);
  if (!top || !bottom) return paths;
  let d = `M${RAIL_X.origin} ${top.y} V${bottom.y}`;
  if (base) {
    const half = (base.y - bottom.y) / 2;
    d += ` C${RAIL_X.origin} ${bottom.y + half} ${RAIL_X.local} ${base.y - half} ${RAIL_X.local} ${base.y}`;
  }
  paths.push({ id: 'origin', rail: 'origin', d });
  return paths;
}

export function layoutHistory(history: RepositoryHistory, tags: readonly TagInfo[] = []): GraphLayout {
  const rows = buildRows(history, tagsByCommit(tags));
  return { rows, paths: railPaths(rows), height: rows.length * ROW_HEIGHT, width: GRAPH_WIDTH };
}

export const ringLabel = (count: number) => (count > 99 ? '99+' : String(count));
export const ringRadius = (count: number) => (count > 99 ? 14 : 10);

export function describeHistory(history: RepositoryHistory): string {
  switch (history.kind) {
    case 'unborn': return 'No commits yet';
    case 'detached': return 'Detached HEAD';
    case 'noUpstream': return 'No upstream branch';
    case 'upstreamGone': return `Upstream ${history.upstream ?? 'branch'} no longer exists`;
    case 'tracking': {
      const upstream = history.upstream ?? 'origin';
      const parts = [history.localTotal && `${history.localTotal} ahead`, history.originTotal && `${history.originTotal} behind`].filter(Boolean);
      return parts.length ? `${parts.join(' · ')} · ${upstream}` : `In sync with ${upstream}`;
    }
  }
}

export function hasSharedBase(history: RepositoryHistory): boolean {
  return history.kind !== 'tracking' || history.base !== null;
}

export function canLoadMore(history: RepositoryHistory): boolean {
  return history.local.length < history.localTotal || history.origin.length < history.originTotal;
}

const dateFormat = new Intl.DateTimeFormat('en-GB', { dateStyle: 'medium', timeStyle: 'short', timeZone: 'Europe/Bucharest' });

export function formatCommitDate(iso: string): string {
  const time = Date.parse(iso);
  return Number.isNaN(time) ? iso : dateFormat.format(time);
}

export function nextRowIndex(key: string, current: number, count: number): number | null {
  if (count === 0) return null;
  if (key === 'ArrowDown') return Math.min(count - 1, current + 1);
  if (key === 'ArrowUp') return Math.max(0, current - 1);
  if (key === 'Home') return 0;
  if (key === 'End') return count - 1;
  return null;
}
