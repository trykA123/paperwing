import './test-support/svelte-loader.js';
import { expect, test } from 'bun:test';
import { deferred, withIpc } from './test-support/ipc-fixture.js';

const { app } = await import('./state.svelte.ts');

const source = { id: 'admin-source', name: 'admin', kind: 'github', host: 'github.com', orgs: ['admin'], urls: [] };
const item = { id: 'admin-item', repoId: 'admin-source:admin/repo', url: 'https://fixture.invalid/admin/repo', ref: { type: 'branch', name: 'main' } };

function stateWithMetadata() {
    const state = new app.constructor();
    state.sources = [source];
    state.repos = {
        [source.id]: [{ id: item.repoId, source: source.id, org: 'admin', name: 'repo', url: item.url, defaultBranch: 'main' }],
        'admin-other': [{ id: 'admin-other:admin/repo' }],
    };
    state.refs = { [item.url]: { branches: ['main'], tags: [] } };
    return state;
}

test('a denied credential observation removes that source listing', async () => {
    await withIpc(command => {
        expect(command).toBe('credential_status');
        return Promise.resolve({ sourceId: source.id, backend: 'secretService', state: 'permissionDenied', revision: 0, reason: 'admin fixture denial' });
    }, async () => {
        const state = stateWithMetadata();
        await state.credentials.refresh(source.id);
        expect(state.repos[source.id]).toBeUndefined();
        expect(state.repos['admin-other']).toEqual([{ id: 'admin-other:admin/repo' }]);
    });
});

test('source invalidation stops its cached references being verified', () => {
    const state = stateWithMetadata();
    expect(state.refState(item)).toBe('ok');
    state.credentials.invalidate(source.id, 1);
    expect(state.refState(item)).toBe('unknown');
});

test('source invalidation rejects its pending older reference response', async () => {
    const response = deferred();
    await withIpc(command => {
        expect(command).toBe('get_refs_many');
        return response.promise;
    }, async () => {
        const state = stateWithMetadata();
        const pending = state.ensureRefs([item.url], true);
        state.credentials.invalidate(source.id, 1);
        response.resolve([{ url: item.url, branches: ['main'], tags: [] }]);
        await pending;
        expect(state.refState(item)).toBe('unknown');
    });
});
