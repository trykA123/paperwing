import './test-support/svelte-loader.js';
import { beforeEach, describe, expect, test } from 'bun:test';
import { deferred } from './test-support/ipc-fixture.js';
import { buildRows, describeRepoStatus, highlightRange, matchLocation } from './search-results.ts';
import { buildSearchRequest, defaultSearchForm, isSearchLimitError, parsePathspecs } from './search-request.ts';

const { SearchSession } = await import('./search.svelte.ts');

const match = (path, line, extra = {}) => ({ path, line, column: 1, text: `line ${line}`, context: [], ...extra });
const done = (id, extra = {}) => ({ id, summary: { repos: 2, matches: 3, failed: 0, capped: false, cancelled: false, ...extra } });
const request = { repos: [{ path: '/r/a', gitRef: null }, { path: '/r/b', gitRef: null }], pattern: 'x' };
const names = { '/r/a': 'a', '/r/b': 'b' };

let handlers, cancelled, started, unsubscribed, release;
const transport = () => ({
  start: async () => { started += 1; return release ? release.promise : 7; },
  cancel: async id => { cancelled.push(id); return true; },
  cancelAll: async () => 0,
  capabilities: async () => ({ perl: false }),
  subscribe: async next => { handlers = next; return () => { unsubscribed = true; }; },
});
const session = () => new SearchSession(transport(), run => run());

beforeEach(() => { handlers = null; cancelled = []; started = 0; unsubscribed = false; release = null; });

describe('events', () => {
  test('buffers events that arrive before the job id and ignores other jobs', async () => {
    release = deferred();
    const search = session();
    const running = search.start(request, names);
    await Promise.resolve(); await Promise.resolve();
    handlers.matches({ id: 7, repo: '/r/a', matches: [match('x.ts', 1)] });
    handlers.matches({ id: 9, repo: '/r/a', matches: [match('other.ts', 1)] });
    release.resolve(7);
    await running;
    handlers.matches({ id: 8, repo: '/r/b', matches: [match('late.ts', 1)] });
    handlers.matches({ id: 7, repo: '/r/b', matches: [match('y.ts', 2)] });
    expect(search.matchCount).toBe(2);
    expect(search.rows.filter(row => row.kind === 'file').map(row => row.path)).toEqual(['x.ts', 'y.ts']);
    expect(search.status).toBe('running');
  });

  test('accepts a repository result before its matches and finishes on done', async () => {
    const search = session();
    await search.start(request, names);
    handlers.repo({ id: 7, repo: '/r/a', status: { state: 'done', matches: 1, truncated: true, error: null } });
    handlers.matches({ id: 7, repo: '/r/a', matches: [match('x.ts', 3)] });
    handlers.done(done(7, { capped: true }));
    expect(search.status).toBe('done');
    expect(search.summary.capped).toBe(true);
    expect(search.reposDone).toBe(1);
    expect(search.rows[0]).toMatchObject({ kind: 'repo', name: 'a', count: 1 });
  });

  test('a cancelled summary ends as cancelled', async () => {
    const search = session();
    await search.start(request, names);
    handlers.done(done(7, { cancelled: true }));
    expect(search.status).toBe('cancelled');
  });
});

describe('lifecycle', () => {
  test('cancel asks the backend to stop the job', async () => {
    const search = session();
    await search.start(request, names);
    await search.cancel();
    expect(cancelled).toEqual([7]);
  });

  test('cancel pressed before the id is known stops the job once it starts', async () => {
    release = deferred();
    const search = session();
    const running = search.start(request, names);
    await Promise.resolve(); await Promise.resolve();
    await search.cancel();
    expect(cancelled).toEqual([]);
    release.resolve(7);
    await running;
    expect(cancelled).toEqual([7]);
  });

  test('closing the tab cancels the running job and stops listening', async () => {
    const search = session();
    await search.start(request, names);
    await search.dispose();
    expect(cancelled).toEqual([7]);
    expect(unsubscribed).toBe(true);
  });

  test('closing the tab while subscribing stops listening and never starts the job', async () => {
    const pending = deferred();
    const search = new SearchSession({ ...transport(), subscribe: async next => { handlers = next; await pending.promise; return () => { unsubscribed = true; }; } }, run => run());
    const running = search.start(request, names);
    await Promise.resolve();
    await search.dispose();
    pending.resolve();
    await running;
    expect(unsubscribed).toBe(true);
    expect(started).toBe(0);
    expect(search.status).toBe('cancelled');
  });

  test('a finished search is not cancelled on close', async () => {
    const search = session();
    await search.start(request, names);
    handlers.done(done(7));
    await search.dispose();
    expect(cancelled).toEqual([]);
  });

  test('the four-search limit error is kept for the view', async () => {
    const search = new SearchSession({ ...transport(), start: async () => { throw new Error('Too many searches are running; cancel one first'); } }, run => run());
    await search.start(request, names);
    expect(search.status).toBe('failed');
    expect(isSearchLimitError(search.error)).toBe(true);
    await search.start(request, names);
    expect(search.status).toBe('failed');
  });

  test('stopping every search frees the slots and starts again once one is free', async () => {
    let attempts = 0, cancelledAll = 0;
    const limited = { ...transport(), cancelAll: async () => { cancelledAll += 1; return 4; }, start: async () => { attempts += 1; if (attempts < 3) throw new Error('Too many searches are running; cancel one first'); return 7; } };
    const search = new SearchSession(limited, run => run());
    await search.restartAfterCancellingAll(request, names, 5);
    expect(cancelledAll).toBe(1);
    expect(attempts).toBe(3);
    expect(search.status).toBe('running');
  });

  test('perl support is read once and defaults to off when it cannot be read', async () => {
    const search = session();
    await search.loadCapabilities();
    expect(search.perl).toBe(false);
  });
});

describe('rows and requests', () => {
  test('groups by repository then file and merges context without repeating match lines', () => {
    const groups = [{ repo: '/r/a', name: 'a', status: null, matches: [
      match('a.ts', 5, { context: [{ line: 4, text: 'before' }, { line: 6, text: 'between' }] }),
      match('a.ts', 6, { context: [{ line: 5, text: 'dup' }, { line: 7, text: 'after' }] }),
      match('b.ts', 1),
    ] }, { repo: '/r/b', name: 'b', status: { state: 'skipped', matches: 0, truncated: false, error: 'Overall result limit reached' }, matches: [] }];
    expect(buildRows(groups).map(row => row.kind === 'match' ? `m${row.match.line}` : row.kind === 'context' ? `c${row.line}` : row.kind === 'file' ? row.path : row.name))
      .toEqual(['a', 'a.ts', 'c4', 'm5', 'm6', 'c7', 'b.ts', 'm1', 'b']);
    expect(new Set(buildRows(groups).map(row => row.key)).size).toBe(9);
  });

  test('finds the matched span from the byte column, with regular expressions too', () => {
    expect(highlightRange(match('a.ts', 1, { text: '// TODO item', column: 4 }), { pattern: 'TODO' })).toEqual([3, 7]);
    expect(highlightRange(match('a.ts', 1, { text: 'é TODO', column: 4 }), { pattern: 'TODO' })).toEqual([2, 6]);
    expect(highlightRange(match('a.ts', 1, { text: 'x = foo123;', column: 5 }), { pattern: 'foo[0-9]+', mode: 'perl' })).toEqual([4, 10]);
    expect(highlightRange(match('a.ts', 1, { text: 'x', column: 1 }), { pattern: '(', mode: 'perl' })).toBeNull();
  });

  test('describes each repository state', () => {
    const status = (state, truncated = false) => ({ state, matches: 0, truncated, error: null });
    expect(['done', 'skipped', 'cancelled', 'failed'].map(state => describeRepoStatus(status(state)).label)).toEqual(['done', 'skipped', 'cancelled', 'failed']);
    expect(describeRepoStatus(status('done', true)).label).toBe('truncated');
  });

  test('builds a request with refs, path filters and a working-tree-only untracked flag', () => {
    const form = { ...defaultSearchForm(), pattern: 'TODO', pathspecs: 'src/*.ts, :!vendor', untracked: true };
    const request = buildSearchRequest([{ path: '/r/a', name: 'a', gitRef: '' }, { path: '/r/b', name: 'b', gitRef: ' v1 ' }], form);
    expect(request.repos).toEqual([{ path: '/r/a', gitRef: null }, { path: '/r/b', gitRef: 'v1' }]);
    expect(request.pathspecs).toEqual(['src/*.ts', ':!vendor']);
    expect(request.untracked).toBe(false);
    expect(parsePathspecs(' a ,\n b')).toEqual(['a', 'b']);
  });

  test('joins the repository and file with the repository separator', () => {
    expect(matchLocation('C:\\Dev\\my repo', match('src/a b.ts', 1))).toBe('C:\\Dev\\my repo\\src\\a b.ts');
    expect(matchLocation('/home/u/r/', match('a.ts', 1))).toBe('/home/u/r/a.ts');
  });
});
