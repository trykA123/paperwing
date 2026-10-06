import './test-support/svelte-loader.js';
import { expect, test } from 'bun:test';
import { deferred, withIpc } from './test-support/ipc-fixture.js';

const { app } = await import('./state.svelte.ts');
const { RepositoryTrees } = await import('./state/repository-trees.svelte.ts');
const { sourceFingerprint, commitHistoryKey } = await import('./state/metadata-keys.ts');

const source = { id: 'admin-source', kind: 'github', host: 'github.com', name: 'admin', orgs: ['admin'], urls: [] };
const repo = { id: 'admin-source:admin/repo', source: source.id, org: 'admin', name: 'repo', url: 'https://fixture.invalid/admin/repo', defaultBranch: 'main' };
const item = { id: 'copy', repoId: repo.id, url: repo.url, ref: { type: 'branch', name: 'main' } };

async function fixture(run) {
  const calls = [];
  await withIpc((command, args) => {
    const response = deferred();
    calls.push({ command, args, ...response });
    return response.promise;
  }, async () => {
    const state = new app.constructor();
    state.sources = [structuredClone(source)];
    state.repos = { [source.id]: [repo] };
    await run(state, calls);
  });
}

test('all foreground metadata owners share identical work when one consumer cancels', async () => {
  for (const kind of ['listing', 'refs', 'commits', 'tree']) {
    await fixture(async (state, calls) => {
      const consumer = new AbortController();
      const load = signal => kind === 'listing' ? state.loadRepos(state.sources[0], false, signal)
        : kind === 'refs' ? state.ensureRefs([item.url], false, signal)
        : kind === 'commits' ? state.ensureCommits(item, false, signal) : state.loadTree('/fixture', false, signal);
      const first = load(consumer.signal), second = load();
      expect(calls).toHaveLength(1);
      consumer.abort();
      await first;
      const result = kind === 'listing' ? { repos: [repo], errors: [] }
        : kind === 'refs' ? [{ url: item.url, branches: ['main'], tags: [] }]
        : kind === 'commits' ? [{ sha: 'abcdef1234' }] : { branches: [{ name: 'main' }] };
      calls[0].resolve(result);
      await second;
      if (kind === 'listing') expect(state.repos[source.id]).toEqual([repo]);
      if (kind === 'refs') expect(state.refState(item)).toBe('ok');
      if (kind === 'commits') expect(state.commitsFor(item)).toEqual(result);
      if (kind === 'tree') expect(state.trees['/fixture'].data).toEqual(result);
    });
  }
});

test('source edits discard old histories and preserve exact branch IPC arguments', async () => {
  await fixture(async (state, calls) => {
    const key = state.commitKey(item), pending = state.ensureCommits(item);
    state.sources[0].host = 'enterprise.invalid';
    const current = state.ensureCommits(item);
    expect(state.commitKey(item)).not.toBe(key);
    expect(calls.map(call => call.args.branch)).toEqual(['main', 'main']);
    calls[1].resolve([{ sha: 'new-host' }]);
    await current;
    calls[0].resolve([{ sha: 'old-host' }]);
    await pending;
    expect(state.commitsFor(item)).toEqual([{ sha: 'new-host' }]);
    expect(state.commits[key]).toBeUndefined();
  });
});

test('ref epochs and force refresh discard late histories without changing selection', async () => {
  await fixture(async (state, calls) => {
    const selected = structuredClone(item), key = state.commitKey(selected);
    const old = state.ensureCommits(selected), fresh = state.ensureCommits(selected, true);
    expect(state.commitKey(selected)).not.toBe(key);
    calls[1].resolve([{ sha: 'current' }]);
    await fresh;
    calls[0].resolve([{ sha: 'obsolete' }]);
    await old;
    expect(state.commitsFor(selected)).toEqual([{ sha: 'current' }]);
    expect(selected.ref).toEqual(item.ref);
    state.setRef(selected, { type: 'branch', name: 'feature' });
    expect(state.commitsFor(selected)).toBeUndefined();
  });
});

test('commit verification reads only the default branch history for commit refs', async () => {
  await fixture(async (state, calls) => {
    const feature = { ...item, ref: { type: 'branch', name: 'feature' } };
    const main = state.ensureCommits(item), other = state.ensureCommits(feature);
    calls[0].resolve([{ sha: 'abcdef0000000000' }]);
    calls[1].resolve([{ sha: '1234567000000000' }]);
    await Promise.all([main, other]);
    expect(state.refState({ ...item, ref: { type: 'commit', name: 'abcdef0' } })).toBe('ok');
    expect(state.refState({ ...item, ref: { type: 'commit', name: '1234567' } })).toBe('unverified');
  });
});

test('denied or unavailable credentials and status errors invalidate permission scoped state', async () => {
  for (const status of ['locked', 'unavailable', 'permissionDenied', 'uncertain', 'missing', 'error']) {
    await withIpc(() => status === 'error' ? Promise.reject(new Error('offline'))
      : { sourceId: source.id, revision: 0, state: status, backend: 'secretService' }, async () => {
      const state = new app.constructor();
      state.sources = [source];
      state.repos = { [source.id]: [repo] };
      state.refs = { [item.url]: { branches: ['main'], tags: [] } };
      await state.credentials.refresh(source.id);
      expect(state.repos[source.id]).toBeUndefined();
      expect(state.refState(item)).toBe('unknown');
    });
  }
});

test('physical root changes and tree errors cannot reuse old path entries', async () => {
  let identity = 'root-a';
  const trees = new RepositoryTrees(() => identity), calls = [];
  await withIpc(() => { const response = deferred(); calls.push(response); return response.promise; }, async () => {
    const old = trees.loadTree('/fixture');
    identity = 'root-b';
    const replacement = trees.loadTree('/fixture');
    calls[1].reject(new Error('retry'));
    await replacement;
    const retry = trees.loadTree('/fixture');
    calls[2].resolve({ identity: 'git-b', branches: ['replacement'] });
    await retry;
    calls[0].resolve({ identity: 'git-a', branches: ['old'] });
    await old;
    expect(trees.trees['/fixture'].data.identity).toBe('git-b');
    expect(calls).toHaveLength(3);
  });
});

test('versioned keys separate owners branches and epochs while checkout copies share histories', () => {
  const scope = { source: 'admin-source', repository: repo.id, branch: 'main', refEpoch: 0 };
  const key = commitHistoryKey(scope);
  for (const changed of [{ source: 'other-source' }, { repository: 'other-repo' }, { branch: 'feature' }, { refEpoch: 1 }]) {
    expect(commitHistoryKey({ ...scope, ...changed })).not.toBe(key);
  }
  expect(sourceFingerprint(source)).not.toBe(sourceFingerprint({ ...source, orgs: ['another'] }));
  expect(sourceFingerprint(source)).not.toBe(sourceFingerprint({ ...source, host: 'enterprise.invalid' }));
  const state = new app.constructor();
  state.sources = [source];
  state.repos = { [source.id]: [repo] };
  expect(state.commitKey(item)).toBe(state.commitKey({ ...item, id: 'another-copy' }));
  state.commits = { [repo.id]: [{ sha: 'legacy' }] };
  expect(state.commitsFor(item)).toBeUndefined();
});

test('credential revisions clear source histories and scoped trees while preserving another source', async () => {
  await fixture(async (state, calls) => {
    state.set.items = [{ ...item, org: 'admin', name: 'repo', on: true }];
    const path = state.dest(state.set.items[0]), key = state.commitKey(item);
    state.trees = { [path]: { data: { identity: 'old' } }, '/unrelated': { data: { identity: 'other' } } };
    const loading = state.ensureCommits(item);
    state.credentials.invalidate(source.id, 1);
    calls[0].resolve([{ sha: 'obsolete' }]);
    await loading;
    expect(state.commits[key]).toBeUndefined();
    expect(state.trees[path]).toBeUndefined();
    expect(state.trees['/unrelated'].data.identity).toBe('other');
  });
});

test('new physical path observations invalidate trees before another consumer can read them', async () => {
  const state = new app.constructor(), path = '/fixture';
  state.pathIdentities = { [path]: { path, identity: 'old', exists: true, reason: null } };
  state.trees = { [path]: { data: { identity: 'old-git' } } };
  await withIpc(() => [{ path, identity: 'new', exists: true, reason: null }], async () => {
    await state.refreshPathIdentities([path]);
    expect(state.trees[path]).toBeUndefined();
  });
});

test('focus invalidates loaded refs and histories while retaining foreground repository browsing', async () => {
  await fixture(async (state, calls) => {
    const loading = state.ensureCommits(item);
    calls[0].resolve([{ sha: 'before-focus' }]);
    await loading;
    state.refs = { [item.url]: { branches: ['main'], tags: [] } };
    state.invalidateMetadata();
    expect(state.commitsFor(item)).toBeUndefined();
    expect(state.refState(item)).toBe('unknown');
    expect(state.repos[source.id]).toEqual([repo]);
    expect(calls).toHaveLength(1);
  });
});

test('saved set references retain source scope when discovery has no repository row', async () => {
  await fixture(async (state, calls) => {
    state.set.items = [item];
    state.repos = {};
    const pending = state.ensureRefs([item.url]);
    state.sources[0].host = 'enterprise.invalid';
    const current = state.ensureRefs([item.url]);
    calls[1].resolve([{ url: item.url, branches: ['main'], tags: [] }]);
    await current;
    calls[0].resolve([{ url: item.url, branches: ['obsolete'], tags: [] }]);
    await pending;
    expect(state.refState(item)).toBe('ok');
    state.credentials.invalidate(source.id, 1);
    expect(state.refState(item)).toBe('unknown');
  });
});

test('tree cache hits require a current native identity and old entries are misses', async () => {
  const trees = new RepositoryTrees(), calls = [];
  await withIpc(() => { const response = deferred(); calls.push(response); return response.promise; }, async () => {
    const first = trees.loadTree('/fixture');
    calls[0].resolve({ identity: 'git-a', branches: [] });
    await first;
    await trees.loadTree('/fixture');
    expect(calls).toHaveLength(1);
    trees.trees['/fixture'] = { data: { branches: ['legacy'] } };
    const fresh = trees.loadTree('/fixture');
    expect(calls).toHaveLength(2);
    calls[1].resolve({ identity: 'git-b', branches: [] });
    await fresh;
    expect(trees.trees['/fixture'].data.identity).toBe('git-b');
  });
});

test('changing one shared history consumer reference preserves the other consumer result', async () => {
  await fixture(async (state, calls) => {
    const firstItem = structuredClone(item), secondItem = { ...structuredClone(item), id: 'another-copy' };
    const consumer = new AbortController();
    const first = state.ensureCommits(firstItem, false, consumer.signal), second = state.ensureCommits(secondItem);
    state.setRef(firstItem, { type: 'branch', name: 'feature' });
    consumer.abort();
    await first;
    calls[0].resolve([{ sha: 'main-result' }]);
    await second;
    expect(calls).toHaveLength(1);
    expect(state.commitsFor(secondItem)).toEqual([{ sha: 'main-result' }]);
    expect(state.commitsFor(firstItem)).toBeUndefined();
  });
});

test('dialog tree reads share picker work and closing the dialog preserves picker results', async () => {
  await fixture(async (state, calls) => {
    const consumer = new AbortController();
    const dialog = state.readTree('/fixture', consumer.signal);
    const rejected = dialog.catch(error => error);
    const picker = state.loadTree('/fixture');
    consumer.abort();
    expect((await rejected).message).toBe('Metadata consumer closed');
    calls[0].resolve({ identity: 'current-git', branches: [] });
    await picker;
    expect(calls).toHaveLength(1);
    expect(state.trees['/fixture'].data.identity).toBe('current-git');
  });
});

test('removing a configured source discards its pending listing and releases the loading state', async () => {
  await fixture(async (state, calls) => {
    state.repos = {};
    const pending = state.loadRepos(state.sources[0], false);
    state.sources = [];
    calls[0].resolve({ repos: [repo], errors: [] });
    await pending;
    expect(state.repos[source.id]).toBeUndefined();
    expect(state.loadingRepos[source.id]).toBe(false);
  });
});

test('changed ref and tree scopes release obsolete loading placeholders', async () => {
  await fixture(async (state, calls) => {
    const refs = state.ensureRefs([item.url]), tree = state.loadTree('/fixture');
    state.sources[0].host = 'enterprise.invalid';
    state.ws.root = '/changed-root';
    calls[0].resolve([{ url: item.url, branches: ['obsolete'], tags: [] }]);
    calls[1].resolve({ identity: 'obsolete', branches: [] });
    await Promise.all([refs, tree]);
    expect(state.refs[item.url]).toBeUndefined();
    expect(state.trees['/fixture']).toBeUndefined();
  });
});

test('missing discovery metadata cannot cache an empty ready history before the repository arrives', async () => {
  await fixture(async (state, calls) => {
    state.repos = {};
    await state.ensureCommits(item);
    expect(calls).toHaveLength(0);
    state.repos = { [source.id]: [repo] };
    const loaded = state.ensureCommits(item);
    expect(calls).toHaveLength(1);
    calls[0].resolve([{ sha: 'discovered' }]);
    await loaded;
    expect(state.commitsFor(item)).toEqual([{ sha: 'discovered' }]);
  });
});
