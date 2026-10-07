import type { StashRestoreOutcome, SwitchStashOutcome } from './api';
import { describeError } from './errors';

export type SwitchTarget = { path: string; name: string; branch: string };
export type SwitchRow = SwitchTarget & { stashed: string | null; switched: boolean; error: string | null };
export type SwitchApi = { switchWithStash: (path: string, branch: string) => Promise<SwitchStashOutcome> };
export type RestoreApi = { stashApply: (path: string, oid: string) => Promise<StashRestoreOutcome> };

export type RestoreState =
  | { phase: 'idle' }
  | { phase: 'working' }
  | { phase: 'applied'; kept: boolean; indexRestored: boolean }
  | { phase: 'conflicted'; files: string[]; message: string }
  | { phase: 'failed'; message: string }
  | { phase: 'dropped' };

/** One repository at a time; a failure is recorded and the loop goes on. */
export async function switchWithStash(targets: SwitchTarget[], api: SwitchApi, onRow?: (index: number, row: SwitchRow) => void): Promise<SwitchRow[]> {
  const rows: SwitchRow[] = [];
  for (const [index, target] of targets.entries()) {
    let row: SwitchRow;
    try {
      const outcome = await api.switchWithStash(target.path, target.branch);
      row = { ...target, stashed: outcome.stashed, switched: outcome.switched, error: outcome.error };
    } catch (reason) {
      row = { ...target, stashed: null, switched: false, error: describeError(reason, `switch ${target.name} to ${target.branch}`) };
    }
    rows.push(row);
    onRow?.(index, row);
  }
  return rows;
}

export type RowStatus = 'switched' | 'switched-stashed' | 'failed' | 'failed-stashed';

export function rowStatus(row: SwitchRow): RowStatus {
  if (row.switched) return row.stashed ? 'switched-stashed' : 'switched';
  return row.stashed ? 'failed-stashed' : 'failed';
}

export const stashedRows = (rows: SwitchRow[]) => rows.filter(row => row.stashed);

/** Apply keeps the stash; the caller offers drop only when the phase is `applied`. */
export async function restoreStash(path: string, oid: string, api: RestoreApi): Promise<RestoreState> {
  try {
    const outcome = await api.stashApply(path, oid);
    if (outcome.applied) return { phase: 'applied', kept: outcome.stashKept, indexRestored: outcome.indexRestored };
    const message = outcome.error ?? 'The stash did not apply.';
    if (outcome.conflicted.length) return { phase: 'conflicted', files: outcome.conflicted, message };
    return { phase: 'failed', message };
  } catch (reason) {
    return { phase: 'failed', message: describeError(reason, 'restore the stash') };
  }
}
