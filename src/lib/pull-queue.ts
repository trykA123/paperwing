import type { PullRequest } from './api';
import type { IconName } from '../components/Icon.svelte';

export type QueueId = 'review' | 'mine' | 'assigned' | 'drafts' | 'open' | 'failing';
export type QueueDef = { id: QueueId; label: string; unavailable?: string };

const NO_AUTHOR = 'Needs the pull request author, which Skein does not read yet';

/** Needs review means open with no review yet; it does not say who was asked. Created by me and Assigned to me wait for the author and assignee fields. */
export const SIDEBAR_QUEUES: readonly QueueDef[] = [
  { id: 'review', label: 'Needs review' },
  { id: 'mine', label: 'Created by me', unavailable: NO_AUTHOR },
  { id: 'assigned', label: 'Assigned to me', unavailable: 'Needs the pull request assignees, which Skein does not read yet' },
  { id: 'drafts', label: 'Drafts' },
  { id: 'open', label: 'All open' },
];

export const CHIP_QUEUES: readonly QueueDef[] = [
  { id: 'open', label: 'All' }, { id: 'review', label: 'Needs review' }, { id: 'drafts', label: 'Drafts' }, { id: 'failing', label: 'Checks failing' },
];

const isLive = (pull: PullRequest) => pull.state === 'open' || pull.state === 'draft';

export function inQueue(queue: QueueId, pull: PullRequest): boolean {
  if (!isLive(pull)) return false;
  switch (queue) {
    case 'open': return true;
    case 'review': return pull.state === 'open' && pull.reviewState === 'reviewRequired';
    case 'drafts': return pull.state === 'draft';
    case 'failing': return pull.checks.state === 'failure';
    case 'mine': case 'assigned': return false;
  }
}

export function queueCounts(pulls: readonly PullRequest[]): Record<QueueId, number> {
  const counts: Record<QueueId, number> = { review: 0, mine: 0, assigned: 0, drafts: 0, open: 0, failing: 0 };
  for (const pull of pulls) for (const id of Object.keys(counts) as QueueId[]) if (inQueue(id, pull)) counts[id] += 1;
  return counts;
}

export type PullStep = { label: string; icon: IconName; title: string };

/** Every step opens the pull request in the browser; the label says what to do there. */
export function pullStep(pull: PullRequest): PullStep {
  if (pull.checks.state === 'failure') return { label: 'View checks', icon: 'external', title: 'Open the pull request to see the failing checks' };
  if (pull.state === 'open' && pull.reviewState === 'reviewRequired') return { label: 'Review', icon: 'eye', title: 'Open the pull request to review it' };
  return { label: 'Open', icon: 'external', title: 'Open the pull request in the browser' };
}

/** Case-insensitive match on the title, number, repository and branch. */
export function matchesText(query: string, fields: readonly string[]): boolean {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const text = fields.join(' ').toLowerCase();
  return words.every(word => text.includes(word));
}

/** A set this size would ask GitHub once per repository, so it waits for a click. */
export const AUTO_LOAD_LIMIT = 50;

/** Loads on its own for a small set, or once the user has asked for this one; never for an empty list. */
export const shouldAutoLoad = ({ count, armed }: { count: number; armed: boolean }) => count > 0 && (count <= AUTO_LOAD_LIMIT || armed);
