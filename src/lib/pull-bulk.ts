import type { CreatedPullRequest, OpenPullRequest, PullRequest } from './api';
import { describeError, redact } from './errors';
import { plural } from './plural';
import { rateLimitText, readPullsError } from './pull-support';

export type BulkInput = {
  id: string; path: string; name: string; head: string | null; ahead: number; hasRemoteBranch: boolean;
  base: string; title: string; existing: PullRequest | null | undefined;
};
export type BulkPlan = BulkInput & { action: 'open' | 'push-open' | 'skip'; reason: string | null };
export type BulkRow = BulkPlan & (
  | { result: 'pending' }
  | { result: 'created'; created: CreatedPullRequest }
  | { result: 'skipped' }
  | { result: 'failed'; message: string }
  | { result: 'stopped'; message: string }
);
export type BulkApi = {
  openPullRequest: (path: string, request: OpenPullRequest) => Promise<CreatedPullRequest>;
  pushBranch: (path: string) => Promise<unknown>;
};
export type BulkOptions = { draft: boolean; pushFirst: boolean };

function skipReason(input: BulkInput): string | null {
  if (!input.head) return 'Not on a branch. Check out a branch first.';
  if (!input.base) return 'Could not find the default branch on the remote.';
  if (input.head === input.base) return `Already on ${input.base}, the branch pull requests target.`;
  if (input.existing && (input.existing.state === 'open' || input.existing.state === 'draft')) return `Already has pull request #${input.existing.number}.`;
  return null;
}

const unpublished = (input: BulkInput) => !input.hasRemoteBranch || input.ahead > 0;

const publishNote = (input: BulkInput) => (input.hasRemoteBranch ? `${plural(input.ahead, 'commit')} not pushed` : 'Branch is not on the remote');

/** Decides per repository; nothing here pushes. Unpublished branches wait for "Push first". */
export function planBulkOpen(inputs: readonly BulkInput[], { pushFirst }: Pick<BulkOptions, 'pushFirst'>): BulkPlan[] {
  return inputs.map(input => {
    const reason = skipReason(input);
    if (reason) return { ...input, action: 'skip', reason };
    if (!unpublished(input)) return { ...input, action: 'open', reason: null };
    if (pushFirst) return { ...input, action: 'push-open', reason: `${publishNote(input)}. It is pushed first.` };
    return { ...input, action: 'skip', reason: `${publishNote(input)}. Tick “Push first” to include it.` };
  });
}

export const initialRows = (plans: readonly BulkPlan[]): BulkRow[] => plans.map(plan => ({ ...plan, result: plan.action === 'skip' ? 'skipped' : 'pending' }));

/** One repository at a time. A failure is recorded and the loop goes on; a rate limit stops it. */
export async function openPullRequests(plans: readonly BulkPlan[], api: BulkApi, options: BulkOptions, onRow?: (index: number, row: BulkRow) => void): Promise<BulkRow[]> {
  const rows = initialRows(plans);
  let stopped: string | null = null;
  for (const [index, plan] of plans.entries()) {
    if (plan.action === 'skip') continue;
    const row: BulkRow = stopped ? { ...plan, result: 'stopped', message: stopped } : await openOne(plan, api, options);
    if (row.result === 'stopped') stopped = row.message;
    rows[index] = row;
    onRow?.(index, row);
  }
  return rows;
}

async function openOne(plan: BulkPlan, api: BulkApi, { draft }: BulkOptions): Promise<BulkRow> {
  if (plan.action === 'push-open') {
    try { await api.pushBranch(plan.path); }
    catch (reason) { return { ...plan, result: 'failed', message: describeError(reason, `push ${plan.name}`) }; }
  }
  try {
    const created = await api.openPullRequest(plan.path, { head: plan.head!, base: plan.base, title: plan.title, body: '', draft });
    return { ...plan, result: 'created', created };
  } catch (reason) {
    const error = readPullsError(reason);
    if (error.kind === 'rateLimited') return { ...plan, result: 'stopped', message: rateLimitText(error.resetAt) };
    return { ...plan, result: 'failed', message: `${plan.name}: ${redact(error.message)}` };
  }
}
