import type { LocalStatus } from './api';

export type FormationFilter = 'all' | 'changes' | 'behind' | 'ahead' | 'notCloned';
export type NextActionKind = 'clone' | 'adopt' | 'commit' | 'switch' | 'diverged' | 'pull' | 'push';
export type NextAction = { kind: NextActionKind; label: string; title: string; aria?: string };
/** `fixedFolder` marks a row for a folder that was opened in place; it is never cloned, switched or pulled by Skein. */
export type RowFacts = { local: LocalStatus | undefined; onRef: boolean; refLabel: string; fixedFolder: boolean; refMissing?: boolean };
export type FilterCounts = Record<FormationFilter, number>;

export const FILTERS: readonly { id: FormationFilter; label: string }[] = [
  { id: 'all', label: 'All' }, { id: 'changes', label: 'Changes' }, { id: 'behind', label: 'Behind' },
  { id: 'ahead', label: 'Ahead' }, { id: 'notCloned', label: 'Not cloned' },
];

const plural = (count: number, word: string) => `${count} ${word}${count === 1 ? '' : 's'}`;

export const isCloned = (local: LocalStatus | undefined) => !!local?.repo && !local.error;
export const isMissing = (local: LocalStatus | undefined) => !!local && !local.exists;

export const isDiverged = (local: LocalStatus | undefined) => isCloned(local) && !!local!.branch && local!.ahead > 0 && local!.behind > 0;

export function nextAction({ local, onRef, refLabel, fixedFolder, refMissing }: RowFacts): NextAction | null {
  if (isMissing(local)) return fixedFolder ? null : { kind: 'clone', label: 'Clone', title: 'Clone this repository' };
  if (!local || !isCloned(local)) return null;
  if (local.dirty > 0) return { kind: 'commit', label: `Commit ${local.dirty}`, title: `Review, stage and commit ${plural(local.dirty, 'changed file')}`, aria: `Commit ${plural(local.dirty, 'changed file')}` };
  if (!onRef && !fixedFolder && !refMissing) return { kind: 'switch', label: 'Switch', title: `Fetch and check out ${refLabel}`, aria: `Switch to ${refLabel}` };
  if (isDiverged(local)) return { kind: 'diverged', label: 'Diverged', title: 'Local and remote both have new commits. Open History to decide how to reconcile.' };
  if (local.branch && local.behind > 0 && !fixedFolder) return { kind: 'pull', label: `Pull ${local.behind}`, title: `Fast-forward ${local.branchLabel ?? local.branch}, ${plural(local.behind, 'commit')} behind` };
  if (local.branch && local.ahead > 0) return { kind: 'push', label: `Push ${local.ahead}`, title: `Push ${plural(local.ahead, 'commit')} to ${local.upstreamLabel ?? local.upstream ?? 'the remote'}` };
  if (local.branch && !local.upstream) return { kind: 'push', label: 'Publish', title: `Push ${local.branchLabel ?? local.branch} to the remote and track it` };
  return null;
}

export function matchesFilter(filter: FormationFilter, local: LocalStatus | undefined): boolean {
  switch (filter) {
    case 'all': return true;
    case 'changes': return isCloned(local) && local!.dirty > 0;
    case 'behind': return isCloned(local) && local!.behind > 0;
    case 'ahead': return isCloned(local) && local!.ahead > 0;
    case 'notCloned': return isMissing(local);
  }
}

export function filterCounts(locals: readonly (LocalStatus | undefined)[]): FilterCounts {
  const counts: FilterCounts = { all: locals.length, changes: 0, behind: 0, ahead: 0, notCloned: 0 };
  for (const local of locals) for (const { id } of FILTERS) if (id !== 'all' && matchesFilter(id, local)) counts[id] += 1;
  return counts;
}

export type SyncView =
  | { kind: 'unknown' }
  | { kind: 'missing' }
  | { kind: 'broken'; reason: string }
  | { kind: 'unavailable'; reason: string }
  | { kind: 'rails'; ahead: number; behind: number; dirty: number; inSync: boolean; unpublished: boolean; label: string };

export function syncView(local: LocalStatus | undefined, failure?: string): SyncView {
  if (!local) return failure ? { kind: 'unavailable', reason: failure } : { kind: 'unknown' };
  if (!local.exists) return { kind: 'missing' };
  if (!local.repo) return { kind: 'broken', reason: 'Folder is not a Git repository' };
  if (local.error) return { kind: 'broken', reason: local.error };
  const { ahead, behind, dirty } = local;
  const parts = [ahead && `${plural(ahead, 'commit')} ahead`, behind && `${plural(behind, 'commit')} behind`, dirty && `${plural(dirty, 'uncommitted file')}`].filter(Boolean);
  const unpublished = !!local.branch && !local.upstream;
  const settled = !ahead && !behind && !dirty;
  return { kind: 'rails', ahead, behind, dirty, inSync: settled && !unpublished, unpublished: settled && unpublished, label: parts.length ? parts.join(', ') : unpublished ? 'Not published' : 'In sync' };
}

export const RAIL_STEP = 14;
export const RAIL_MAX_DOTS = 4;
export const shownDots = (count: number) => Math.max(0, Math.min(count, RAIL_MAX_DOTS));

export type BulkTargets<T> = { cloned: T[]; fetchable: T[]; behind: T[]; offRef: T[]; pushable: T[] };

export function bulkTargets<T>(items: readonly T[], read: (item: T) => RowFacts): BulkTargets<T> {
  const cloned = items.filter(item => isCloned(read(item).local));
  const fetchable = cloned.filter(item => !read(item).fixedFolder);
  const behind = cloned.filter(item => { const facts = read(item); return facts.local!.behind > 0 && !facts.fixedFolder && !isDiverged(facts.local); });
  const offRef = cloned.filter(item => { const facts = read(item); return !facts.onRef && !facts.fixedFolder && !facts.refMissing; });
  const pushable = cloned.filter(item => { const l = read(item).local!; return !!l.branch && (l.ahead > 0 || !l.upstream); });
  return { cloned, fetchable, behind, offRef, pushable };
}
