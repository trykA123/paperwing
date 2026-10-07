import type { BranchOutcome, MergedBranches } from './api';

export type CleanupTarget = { path: string; name: string };
export type CleanupRow = {
  name: string; oid: string; subject: string; lastCommit: number;
  /** Shown beside the name, never a reason to block. */
  notes: string[];
  /** Why the branch cannot be selected; null when it can. */
  blocked: string | null;
  preselect: boolean;
};
export type CleanupResult = { target: CleanupTarget; deleted: string[]; failed: BranchOutcome[]; error: string | null };
export type CleanupClient = {
  deleteMergedBranches: (path: string, names: string[], expected: string[], base?: string | null) => Promise<BranchOutcome[]>;
};

export const SQUASH_NOTE = 'Squash-merged branches are not detected. Only branches Git sees as merged are listed.';
const PROTECTED = ['main', 'master'];

function protectedNames(data: MergedBranches): Set<string> {
  const remoteBase = data.remote && data.remoteBase?.startsWith(`${data.remote}/`) ? data.remoteBase.slice(data.remote.length + 1) : data.remoteBase;
  return new Set([...PROTECTED, data.baseName, ...(remoteBase ? [remoteBase] : [])]);
}

export function localRows(data: MergedBranches): CleanupRow[] {
  const guarded = protectedNames(data);
  return data.local.map(branch => {
    const blocked = branch.name === data.current ? 'Current branch'
      : guarded.has(branch.name) ? 'Protected'
      : branch.inWorktree ? 'Checked out in a worktree'
      : !branch.merged ? `Not merged into ${data.base}`
      : null;
    const notes = branch.upstreamGone ? ['Upstream gone'] : [];
    return { name: branch.name, oid: branch.oid, subject: branch.subject, lastCommit: branch.lastCommit, notes, blocked, preselect: !blocked && !branch.upstreamGone };
  });
}

export const defaultSelection = (rows: CleanupRow[]): string[] => rows.filter(row => row.preselect).map(row => row.name);

export function selectableNames(rows: CleanupRow[], names: string[]): string[] {
  const allowed = new Set(rows.filter(row => !row.blocked).map(row => row.name));
  return names.filter(name => allowed.has(name));
}

export function expectedTips(rows: CleanupRow[], names: string[]): string[] {
  const tips = new Map(rows.map(row => [row.name, row.oid]));
  return names.map(name => tips.get(name) ?? '');
}

function settle(target: CleanupTarget, names: string[], outcomes: BranchOutcome[]): CleanupResult {
  const byName = new Map(outcomes.map(outcome => [outcome.name, outcome]));
  const failed = names.map(name => byName.get(name) ?? { name, deleted: false, error: 'No result was reported' }).filter(outcome => !outcome.deleted);
  return { target, deleted: names.filter(name => byName.get(name)?.deleted), failed, error: null };
}

const failure = (target: CleanupTarget, what: string, reason: unknown): CleanupResult => ({ target, deleted: [], failed: [], error: `${what} in ${target.name}: ${reason instanceof Error ? reason.message : String(reason)}` });

export type LocalDelete = { target: CleanupTarget; data: MergedBranches; rows: CleanupRow[]; names: string[] };

/** Runs repository by repository; a failing repository never stops the next one. */
export async function deleteLocalAcross(client: CleanupClient, entries: LocalDelete[]): Promise<CleanupResult[]> {
  const results: CleanupResult[] = [];
  for (const { target, data, rows, names } of entries) {
    const picked = selectableNames(rows, names);
    if (!picked.length) continue;
    try { results.push(settle(target, picked, await client.deleteMergedBranches(target.path, picked, expectedTips(rows, picked), data.base))); }
    catch (reason) { results.push(failure(target, 'Could not delete branches', reason)); }
  }
  return results;
}

export function summarizeCleanup(results: CleanupResult[]): { deleted: number; failed: number } {
  return { deleted: results.reduce((sum, result) => sum + result.deleted.length, 0), failed: results.reduce((sum, result) => sum + result.failed.length + (result.error ? 1 : 0), 0) };
}

const DATE = new Intl.DateTimeFormat('en-GB', { timeZone: 'Europe/Bucharest', day: 'numeric', month: 'short', year: 'numeric' });
export const formatCommitDate = (unix: number): string => (unix > 0 ? DATE.format(new Date(unix * 1000)) : '');
