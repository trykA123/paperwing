import './test-support/svelte-loader.js';
import { describe, expect, test } from 'bun:test';
import { baseChoices, defaultTitle } from './pull-defaults.ts';
import { initialRows, openPullRequests, planBulkOpen } from './pull-bulk.ts';
import { pullChip } from './pull-chip.ts';
import { formatReset, rateLimitText, readPullsError } from './pull-support.ts';

const { PullLoader } = await import('./pulls.svelte.ts');

const pull = (patch = {}) => ({
  number: 7, title: 'Add login', url: 'https://ghe.example.test/o/r/pull/7', state: 'open', base: 'main', headSha: 'a'.repeat(40),
  reviewState: 'none', checks: { state: 'none', success: 0, failure: 0, pending: 0, total: 0 }, targetRepo: 'o/r', hasUnpushedCommits: false, ...patch,
});
const key = name => ({ path: `/r/${name}`, branch: 'feature/login' });
const settle = () => new Promise(resolve => setTimeout(resolve, 0));
const waitFor = async condition => { for (let tries = 0; tries < 200 && !condition(); tries += 1) await settle(); };

function fakeApi(handler = () => pull()) {
  const state = { calls: [], active: 0, peak: 0, gates: [] };
  return Object.assign(state, {
    pullForBranch: async (path, branch) => {
      state.calls.push([path, branch]);
      state.active += 1; state.peak = Math.max(state.peak, state.active);
      try { await new Promise(resolve => state.gates.push(resolve)); return handler(path, branch); }
      finally { state.active -= 1; }
    },
    release: () => { while (state.gates.length) state.gates.shift()(); },
  });
}

describe('pull loader', () => {
  test('asks once per row and serves the second ask from the cache', async () => {
    const api = fakeApi();
    const loader = new PullLoader(api);
    loader.want(key('a')); loader.want(key('a'));
    api.release();
    await waitFor(() => loader.entry(key('a'))?.status === 'ready');
    loader.want(key('a'));
    expect(api.calls).toEqual([['/r/a', 'feature/login']]);
    expect(loader.entry(key('a'))).toEqual({ status: 'ready', pull: pull() });
  });

  test('runs four requests at a time', async () => {
    const api = fakeApi();
    const loader = new PullLoader(api);
    const names = Array.from({ length: 10 }, (_, index) => `r${index}`);
    for (const name of names) loader.want(key(name));
    await waitFor(() => api.active === 4);
    expect(api.peak).toBe(4);
    while (names.some(name => loader.entry(key(name))?.status === 'loading')) { api.release(); await settle(); }
    expect(api.calls).toHaveLength(10);
    expect(api.peak).toBe(4);
  });

  test('rows that left the screen before their turn are never requested', async () => {
    const api = fakeApi();
    const loader = new PullLoader(api, { concurrency: 1 });
    const release = ['a', 'b', 'c'].map(name => loader.want(key(name)));
    await waitFor(() => api.active === 1);
    release[1](); release[2]();
    api.release();
    await waitFor(() => loader.entry(key('a'))?.status === 'ready');
    await settle();
    expect(api.calls).toEqual([['/r/a', 'feature/login']]);
    expect(loader.entry(key('b'))).toBeUndefined();
  });

  test('selected rows load although they are not on screen', async () => {
    const api = fakeApi();
    const loader = new PullLoader(api);
    loader.pin([key('far')]);
    api.release();
    await waitFor(() => loader.entry(key('far'))?.status === 'ready');
    expect(api.calls).toHaveLength(1);
  });

  test('a rate limit stops the batch, shows the reset time and never retries', async () => {
    const resetAt = '2026-10-07T12:30:00Z';
    const seen = [];
    const api = fakeApi(() => { throw { kind: 'rateLimited', resetAt, message: 'limit' }; });
    const loader = new PullLoader(api, { concurrency: 1, now: () => Date.parse('2026-10-07T12:00:00Z'), onRateLimit: limit => seen.push(limit) });
    for (const name of ['a', 'b', 'c']) loader.want(key(name));
    await waitFor(() => api.active === 1);
    api.release();
    await waitFor(() => loader.limit !== null);
    await settle();
    expect(api.calls).toHaveLength(1);
    expect(loader.entries).toEqual({});
    expect(seen).toEqual([{ resetAt, message: 'limit' }]);
    loader.want(key('d'));
    await settle();
    expect(api.calls).toHaveLength(1);
    expect(rateLimitText(resetAt, Date.parse('2026-10-07T12:00:00Z'))).toContain('15:30');
  });

  test('requests resume after the reset time has passed', async () => {
    let now = Date.parse('2026-10-07T12:00:00Z');
    let limited = true;
    const api = fakeApi(() => { if (limited) throw { kind: 'rateLimited', resetAt: '2026-10-07T12:30:00Z', message: 'limit' }; return pull(); });
    const loader = new PullLoader(api, { now: () => now });
    loader.want(key('a'));
    await waitFor(() => api.active === 1);
    api.release();
    await waitFor(() => loader.limit !== null);
    now = Date.parse('2026-10-07T12:31:00Z'); limited = false;
    loader.want(key('b'));
    api.release();
    await waitFor(() => loader.entry(key('b'))?.status === 'ready');
    expect(loader.limit).toBeNull();
  });

  test('a missing source is kept as the row message', async () => {
    const api = fakeApi(() => { throw { kind: 'message', message: 'Add a source for ghe.example.test' }; });
    const loader = new PullLoader(api);
    loader.want(key('a'));
    await waitFor(() => api.active === 1);
    api.release();
    await waitFor(() => loader.entry(key('a'))?.status === 'failed');
    expect(loader.entry(key('a'))).toEqual({ status: 'failed', message: 'Add a source for ghe.example.test' });
  });

  test('refresh asks again for rows on screen', async () => {
    const api = fakeApi();
    const loader = new PullLoader(api);
    loader.want(key('a'));
    api.release();
    await waitFor(() => loader.entry(key('a'))?.status === 'ready');
    loader.refresh();
    api.release();
    await waitFor(() => api.calls.length === 2);
    expect(api.calls).toHaveLength(2);
  });

  test('counts open pull requests that wait for a review', async () => {
    const api = fakeApi(path => pull({ reviewState: path.endsWith('a') ? 'reviewRequired' : 'approved' }));
    const loader = new PullLoader(api);
    loader.pin([key('a'), key('b')]);
    await waitFor(() => api.active === 2);
    api.release();
    await waitFor(() => loader.entry(key('b'))?.status === 'ready');
    expect(loader.awaitingReview).toBe(1);
  });
});

describe('errors and times', () => {
  test('commands reject with an object, a string or an Error', () => {
    expect(readPullsError({ kind: 'rateLimited', resetAt: 'x', message: 'm' }).kind).toBe('rateLimited');
    expect(readPullsError({ kind: 'message', message: 'Add a source for h' })).toEqual({ kind: 'message', message: 'Add a source for h' });
    expect(readPullsError('plain')).toEqual({ kind: 'message', message: 'plain' });
    expect(readPullsError(new Error('boom'))).toEqual({ kind: 'message', message: 'boom' });
  });

  test('reset times show in Europe/Bucharest, with the date only on another day', () => {
    const now = Date.parse('2026-10-07T12:00:00Z');
    expect(formatReset('2026-10-07T12:30:00Z', now)).toBe('15:30');
    expect(formatReset('2026-10-08T08:00:00Z', now)).toBe('8 Oct, 11:00');
  });
});

describe('chip', () => {
  test('open pull request shows review and checks', () => {
    const chip = pullChip(pull({ reviewState: 'approved', checks: { state: 'failure', success: 3, failure: 2, pending: 0, total: 5 } }));
    expect([chip.number, chip.state.label, chip.review?.label, chip.checks?.label]).toEqual(['#7', 'Open', 'Approved', '2 failing']);
  });

  test('merged and closed pull requests hide review and checks', () => {
    const merged = pullChip(pull({ state: 'merged', reviewState: 'approved', checks: { state: 'success', success: 1, failure: 0, pending: 0, total: 1 } }));
    expect([merged.state.label, merged.review, merged.checks]).toEqual(['Merged', null, null]);
    expect(pullChip(pull({ state: 'draft' })).state.label).toBe('Draft');
  });
});

describe('defaults', () => {
  const ref = (name, symbolic = '') => ({ name, sha: 'a', symbolic, current: false });
  const tree = refs => ({ remotes: [{ name: 'origin', urls: [], refs }] });

  test('base is the remote HEAD, then main, then master', () => {
    expect(baseChoices(tree([ref('origin/HEAD', 'refs/remotes/origin/develop'), ref('origin/main'), ref('origin/develop')]))).toEqual({ names: ['develop', 'main'], defaultBase: 'develop' });
    expect(baseChoices(tree([ref('origin/zeta'), ref('origin/main')])).defaultBase).toBe('main');
    expect(baseChoices(tree([ref('origin/master'), ref('origin/zeta')])).defaultBase).toBe('master');
    expect(baseChoices(tree([ref('origin/zeta')])).defaultBase).toBe('zeta');
    expect(baseChoices({ remotes: [] })).toEqual({ names: [], defaultBase: '' });
  });

  test('title is the newest commit subject, else the branch', () => {
    expect(defaultTitle({ local: [{ subject: ' Add login ' }] }, 'feature/login')).toBe('Add login');
    expect(defaultTitle({ local: [] }, 'feature/login')).toBe('feature/login');
    expect(defaultTitle(null, 'feature/login')).toBe('feature/login');
  });
});

describe('bulk open', () => {
  const input = (name, patch = {}) => ({ id: name, path: `/r/${name}`, name, head: 'feature/login', ahead: 0, hasRemoteBranch: true, base: 'main', title: `Title ${name}`, existing: null, ...patch });
  const created = name => ({ number: 1, url: `https://x/${name}/pull/1`, targetRepo: `o/${name}`, hasUnpushedCommits: false });
  const fake = (handlers = {}) => {
    const calls = [];
    return { calls, pushBranch: async path => { calls.push(['push', path]); await handlers.push?.(path); }, openPullRequest: async (path, request) => { calls.push(['open', path, request]); return handlers.open ? handlers.open(path) : created(path.split('/').pop()); } };
  };
  const options = { draft: true, pushFirst: false };

  test('skip rules: detached, on the base branch, existing pull request, unpublished', () => {
    const plans = planBulkOpen([
      input('detached', { head: null }), input('onbase', { head: 'main' }), input('has', { existing: { number: 4, state: 'draft' } }),
      input('local', { hasRemoteBranch: false }), input('ahead', { ahead: 2 }), input('merged', { existing: { number: 3, state: 'merged' } }), input('ok'),
    ], { pushFirst: false });
    expect(plans.map(plan => plan.action)).toEqual(['skip', 'skip', 'skip', 'skip', 'skip', 'open', 'open']);
    expect(plans[2].reason).toContain('#4');
    expect(plans[4].reason).toContain('2 commits not pushed');
  });

  test('Push first turns unpublished rows into push then open', () => {
    const plans = planBulkOpen([input('local', { hasRemoteBranch: false }), input('ok'), input('detached', { head: null })], { pushFirst: true });
    expect(plans.map(plan => plan.action)).toEqual(['push-open', 'open', 'skip']);
  });

  test('never pushes without Push first', async () => {
    const api = fake();
    await openPullRequests(planBulkOpen([input('a', { ahead: 1 }), input('b')], { pushFirst: false }), api, options);
    expect(api.calls.map(call => call[0])).toEqual(['open']);
  });

  test('pushes before opening when asked', async () => {
    const api = fake();
    const rows = await openPullRequests(planBulkOpen([input('a', { ahead: 1 })], { pushFirst: true }), api, { draft: false, pushFirst: true });
    expect(api.calls.map(call => call[0])).toEqual(['push', 'open']);
    expect(api.calls[1][2]).toEqual({ head: 'feature/login', base: 'main', title: 'Title a', body: '', draft: false });
    expect(rows[0].result).toBe('created');
  });

  test('a failed push skips the open', async () => {
    const api = fake({ push: () => { throw 'rejected'; } });
    const rows = await openPullRequests(planBulkOpen([input('a', { ahead: 1 })], { pushFirst: true }), api, options);
    expect(api.calls.map(call => call[0])).toEqual(['push']);
    expect(rows[0].result).toBe('failed');
  });

  test('one failure among three does not stop the others', async () => {
    const api = fake({ open: path => { if (path.endsWith('b')) throw { kind: 'message', message: 'Add a source for ghe.example.test' }; return created(path.split('/').pop()); } });
    const seen = [];
    const rows = await openPullRequests(planBulkOpen([input('a'), input('b'), input('c')], { pushFirst: false }), api, options, index => seen.push(index));
    expect(rows.map(row => row.result)).toEqual(['created', 'failed', 'created']);
    expect(rows[1].message).toContain('Add a source for ghe.example.test');
    expect(seen).toEqual([0, 1, 2]);
  });

  test('a rate limit stops the rest and says when it resets', async () => {
    const api = fake({ open: path => { if (path.endsWith('b')) throw { kind: 'rateLimited', resetAt: '2099-01-01T12:00:00Z', message: 'limit' }; return created('a'); } });
    const rows = await openPullRequests(planBulkOpen([input('a'), input('b'), input('c')], { pushFirst: false }), api, options);
    expect(rows.map(row => row.result)).toEqual(['created', 'stopped', 'stopped']);
    expect(rows[2].message).toContain('rate limit');
    expect(api.calls.filter(call => call[0] === 'open')).toHaveLength(2);
    expect(initialRows(planBulkOpen([input('a', { head: null })], { pushFirst: false }))[0].result).toBe('skipped');
  });
});

describe('prepare', () => {
  test('a failed read leaves the field empty and keeps the rest', async () => {
    const { preparePull, mapLimit } = await import('./pull-defaults.ts');
    const tree = { remotes: [{ name: 'origin', urls: [], refs: [{ name: 'origin/main', sha: 'a', symbolic: '', current: false }] }] };
    const prepared = await preparePull('/r/a', 'feature/x', { tree: async () => tree, history: async () => { throw 'no history'; } });
    expect(prepared).toEqual({ names: ['main'], base: 'main', title: 'feature/x' });
    const empty = await preparePull('/r/a', 'feature/x', { tree: async () => { throw 'no tree'; }, history: async () => ({ local: [{ subject: 'Fix' }] }) });
    expect(empty).toEqual({ names: [], base: '', title: 'Fix' });
  });

  test('mapLimit keeps order and never exceeds its limit', async () => {
    const { mapLimit } = await import('./pull-defaults.ts');
    let active = 0, peak = 0;
    const out = await mapLimit([1, 2, 3, 4, 5, 6, 7], 3, async value => { active += 1; peak = Math.max(peak, active); await settle(); active -= 1; return value * 2; });
    expect(out).toEqual([2, 4, 6, 8, 10, 12, 14]);
    expect(peak).toBe(3);
  });
});

describe('selection', () => {
  test('selecting a large set loads only the first fifty rows ahead of scrolling', async () => {
    const api = fakeApi();
    const loader = new PullLoader(api, { concurrency: 100 });
    loader.pin(Array.from({ length: 800 }, (_, index) => key(`r${index}`)));
    await waitFor(() => api.active === 50);
    expect(api.calls).toHaveLength(50);
  });
});
