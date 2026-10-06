import './test-support/svelte-loader.js';
import { expect, test } from 'bun:test';
import { deferred, withIpc } from './test-support/ipc-fixture.js';

const { app } = await import('./state.svelte.ts');
const { Credentials, credentialLabel } = await import('./state/credentials.svelte.ts');

test('Credential invalidation rejects late repository and commit responses for its source only', async () => {
  const pending = [];
  await withIpc(() => {
    const response = deferred();
    pending.push(response);
    return response.promise;
  }, async () => {
    const state = new app.constructor();
    const source = { id: 'source', kind: 'github', name: 'Fixture' };
    state.sources = [source];
    state.repos = {
      source: [{ id: 'source:repo', source: 'source', org: 'org', name: 'repo', defaultBranch: 'main' }],
      other: [{ id: 'other:repo', source: 'other' }],
    };
    state.commits = { 'other:repo': [{ sha: 'other-sha' }] };
    const repositories = state.loadRepos(source, false);
    const commits = state.ensureCommits({ repoId: 'source:repo', ref: { type: 'branch', name: 'main' } });
    state.repositoryMetadata.invalidateSource('source');
    pending[0].resolve({ repos: [{ id: 'stale' }], errors: ['stale'] });
    pending[1].resolve([{ sha: 'stale' }]);
    await Promise.all([repositories, commits]);
    expect(state.repos.source).toBeUndefined();
    expect(state.repoErrors.source).toBeUndefined();
    expect(state.commits['source:repo']).toBeUndefined();
    expect(state.repos.other).toEqual([{ id: 'other:repo', source: 'other' }]);
    expect(state.commits['other:repo']).toEqual([{ sha: 'other-sha' }]);
    expect(state.loadingRepos.source).toBe(false);
  });
});

test('A late credential status cannot restore the saved flag after a newer locked result', async () => {
  const pending = [];
  await withIpc(() => {
    const response = deferred();
    pending.push(response);
    return response.promise;
  }, async () => {
    const credentials = new Credentials(() => {});
    const older = credentials.refresh('source'), newer = credentials.refresh('source');
    pending[1].resolve({ sourceId: 'source', backend: 'secretService', state: 'locked', revision: 2, reason: 'Unlock the wallet' });
    await newer;
    pending[0].resolve({ sourceId: 'source', backend: 'secretService', state: 'saved', revision: 1, reason: null });
    await older;
    expect(credentials.statuses.source.state).toBe('locked');
    expect(credentialLabel(credentials.statuses.source)).toBe('credential store locked');
  });
});

test('A failed credential mutation invalidates only its source and keeps the uncertain result', async () => {
  const invalidated = [], calls = [];
  await withIpc((command, args) => {
    calls.push({ command, args });
    if (command === 'set_token') return Promise.reject('The token may have changed');
    return { sourceId: 'source', backend: 'secretService', state: 'uncertain', revision: 1, reason: 'Explicitly retry' };
  }, async () => {
    const credentials = new Credentials(id => invalidated.push(id));
    await expect(credentials.mutate('source', 'in-memory-only-test-input')).rejects.toBe('The token may have changed');
    expect(invalidated).toEqual(['source', 'source']);
    expect(calls.map(call => call.command)).toEqual(['set_token', 'credential_status']);
    expect(credentials.statuses.source.state).toBe('uncertain');
    expect(credentialLabel(credentials.statuses.source)).toBe('token outcome uncertain');
    expect(JSON.stringify(credentials.statuses)).not.toContain('in-memory-only-test-input');
    credentials.invalidate('source', 1);
    credentials.invalidate('source', 1);
    expect(invalidated).toEqual(['source', 'source']);
    credentials.invalidate('source', 2);
    expect(invalidated).toEqual(['source', 'source', 'source']);
  });
});

test('Synchronizing a saved source revision rejects duplicate delayed invalidation events', async () => {
  const invalidated = [];
  await withIpc(command => {
    expect(command).toBe('source_revision');
    return 3;
  }, async () => {
    const credentials = new Credentials(id => invalidated.push(id));
    await credentials.synchronize('source');
    credentials.invalidate('source', 3);
    credentials.invalidate('source', 2);
    expect(invalidated).toEqual(['source']);
  });
});
