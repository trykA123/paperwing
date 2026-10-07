const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import type { PullRequest } from './api';
import { CHIP_QUEUES, inQueue, matchesText, pullStep, queueCounts, shouldAutoLoad, SIDEBAR_QUEUES } from './pull-queue';

const none = { state: 'none', success: 0, failure: 0, pending: 0, total: 0 } as const;
const pull = (patch: Partial<PullRequest> = {}): PullRequest => ({ number: 1, title: 'Retry 429', url: 'u', state: 'open', base: 'main', headSha: 'a', reviewState: 'none', checks: none, targetRepo: 'o/r', hasUnpushedCommits: false, ...patch });
const failing = { state: 'failure', success: 1, failure: 2, pending: 0, total: 3 } as const;

describe('queues', () => {
  test('needs review is an open pull request without a review yet', () => {
    expect(inQueue('review', pull({ reviewState: 'reviewRequired' }))).toBe(true);
    expect(inQueue('review', pull({ state: 'draft', reviewState: 'reviewRequired' }))).toBe(false);
    expect(inQueue('review', pull({ reviewState: 'approved' }))).toBe(false);
  });

  test('merged and closed pull requests are in no queue', () => {
    for (const state of ['merged', 'closed'] as const) for (const { id } of [...SIDEBAR_QUEUES, ...CHIP_QUEUES]) expect(inQueue(id, pull({ state }))).toBe(false);
  });

  test('all open holds open and draft; drafts hold drafts; failing holds failing checks', () => {
    expect(inQueue('open', pull({ state: 'draft' }))).toBe(true);
    expect(inQueue('drafts', pull({ state: 'draft' }))).toBe(true);
    expect(inQueue('drafts', pull())).toBe(false);
    expect(inQueue('failing', pull({ checks: failing }))).toBe(true);
    expect(inQueue('failing', pull())).toBe(false);
  });

  test('queues that need author data stay empty and say why', () => {
    expect(inQueue('mine', pull())).toBe(false);
    expect(SIDEBAR_QUEUES.filter(queue => queue.unavailable).map(queue => queue.id)).toEqual(['mine', 'assigned']);
  });

  test('counts every queue from one list', () => {
    const counts = queueCounts([pull({ reviewState: 'reviewRequired' }), pull({ state: 'draft' }), pull({ checks: failing }), pull({ state: 'merged' })]);
    expect(counts).toEqual({ review: 1, mine: 0, assigned: 0, drafts: 1, open: 3, failing: 1 });
  });
});

describe('row text and steps', () => {
  test('the step says what to do in the browser', () => {
    expect(pullStep(pull({ checks: failing })).label).toBe('View checks');
    expect(pullStep(pull({ reviewState: 'reviewRequired' })).label).toBe('Review');
    expect(pullStep(pull({ reviewState: 'approved' })).label).toBe('Open');
  });

  test('the filter box needs every word somewhere in the row', () => {
    expect(matchesText('retry gateway', ['Retry 429', '#4182', 'payments/gateway-etl'])).toBe(true);
    expect(matchesText('retry billing', ['Retry 429', 'payments/gateway-etl'])).toBe(false);
    expect(matchesText('  ', ['anything'])).toBe(true);
  });
});

describe('loading', () => {
  test('a small set loads on its own, so keys that appear later load too', () => {
    expect(shouldAutoLoad({ count: 0, armed: false })).toBe(false);
    expect(shouldAutoLoad({ count: 1, armed: false })).toBe(true);
    expect(shouldAutoLoad({ count: 50, armed: false })).toBe(true);
  });

  test('a large set waits for a click, then keeps loading', () => {
    expect(shouldAutoLoad({ count: 51, armed: false })).toBe(false);
    expect(shouldAutoLoad({ count: 800, armed: true })).toBe(true);
    expect(shouldAutoLoad({ count: 0, armed: true })).toBe(false);
  });

  test('the queue labels never say whose review', () => {
    for (const queue of [...SIDEBAR_QUEUES, ...CHIP_QUEUES]) expect(queue.label).not.toMatch(/my review|awaiting/i);
  });
});

