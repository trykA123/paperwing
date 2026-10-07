import type { PullRequest } from './api';
import type { IconName } from '../components/Icon.svelte';
import { plural } from './plural';

export type ChipPart = { label: string; tone: 'ok' | 'warn' | 'err' | 'info' | 'dim'; icon?: IconName; title: string };
export type ChipView = { number: string; state: ChipPart; review: ChipPart | null; checks: ChipPart | null; title: string };

const STATE = {
  open: { label: 'Open', tone: 'ok' },
  draft: { label: 'Draft', tone: 'dim' },
  merged: { label: 'Merged', tone: 'info' },
  closed: { label: 'Closed', tone: 'err' },
} as const;

function reviewPart(pull: PullRequest): ChipPart | null {
  switch (pull.reviewState) {
    case 'approved': return { label: 'Approved', tone: 'ok', icon: 'check', title: 'Approved by reviewers' };
    case 'changesRequested': return { label: 'Changes', tone: 'err', icon: 'alert', title: 'A reviewer requested changes' };
    case 'reviewRequired': return { label: 'Review', tone: 'warn', icon: 'info', title: 'Waiting for a review' };
    case 'none': return null;
  }
}

function checksPart(pull: PullRequest): ChipPart | null {
  const { state, success, failure, pending, total } = pull.checks;
  switch (state) {
    case 'success': return { label: `${success}/${total}`, tone: 'ok', icon: 'check', title: `Checks passed: ${success} of ${plural(total, 'check')}` };
    case 'failure': return { label: `${failure} failing`, tone: 'err', icon: 'error', title: `Checks failing: ${failure} of ${plural(total, 'check')}` };
    case 'pending': return { label: `${pending} running`, tone: 'warn', icon: 'refresh', title: `Checks running: ${pending} of ${plural(total, 'check')}` };
    case 'none': return null;
  }
}

/** What the chip shows. Review and checks only matter while the pull request can still change. */
export function pullChip(pull: PullRequest): ChipView {
  const base = STATE[pull.state];
  const live = pull.state === 'open' || pull.state === 'draft';
  const state: ChipPart = { ...base, title: `${base.label} pull request` };
  return {
    number: `#${pull.number}`, state, review: live ? reviewPart(pull) : null, checks: live ? checksPart(pull) : null,
    title: `#${pull.number} ${pull.title} · ${state.label} · into ${pull.base} on ${pull.targetRepo}`,
  };
}

const SOURCE_PREFIX = /^Add a source for /;

/** What a row shows when its request failed; a missing source keeps its own words. */
export function failureView(message: string): { label: string; title: string } {
  return { label: SOURCE_PREFIX.test(message) ? 'Add a source' : 'Unavailable', title: message };
}
