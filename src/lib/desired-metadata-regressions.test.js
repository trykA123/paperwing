import './test-support/svelte-loader.js';
import { expect, test } from 'bun:test';
import { deferred, withIpc } from './test-support/ipc-fixture.js';

const { app } = await import('./state.svelte.ts');

function source() {
    return { id: 'admin-source', name: 'admin', kind: 'github', host: '', orgs: ['admin'], urls: [] };
}

function item(branch) {
    return { id: 'admin-item', repoId: 'admin-source:admin/repo', url: 'https://fixture.invalid/admin/repo', ref: { type: 'branch', name: branch } };
}

async function fixture(run) {
    const calls = [];
    await withIpc((command, args) => {
        const response = deferred();
        calls.push({ command, args, ...response });
        return response.promise;
    }, async () => {
        const state = new app.constructor();
        const configured = source();
        state.sources = [configured];
        state.repos = {
            [configured.id]: [{ id: item('main').repoId, source: configured.id, org: 'admin', name: 'repo', defaultBranch: 'main' }],
        };
        await run(state, calls, configured);
    });
}

test('a completed forced source refresh rejects an older successful response', async () => {
    await fixture(async (state, calls, configured) => {
        const older = state.loadRepos(configured, false);
        const newer = state.loadRepos(configured, true);
        calls[1].resolve({ repos: [{ id: 'admin-newer' }], errors: [] });
        await newer;
        calls[0].resolve({ repos: [{ id: 'admin-older' }], errors: ['older fixture warning'] });
        await older;
        expect(state.repos[configured.id]).toEqual([{ id: 'admin-newer' }]);
        expect(state.repoErrors[configured.id]).toEqual([]);
    });
});

test('an older source failure cannot mark a completed newer refresh erroneous', async () => {
    await fixture(async (state, calls, configured) => {
        const older = state.loadRepos(configured, false);
        const newer = state.loadRepos(configured, true);
        calls[1].resolve({ repos: [{ id: 'admin-newer' }], errors: [] });
        await newer;
        calls[0].reject(new Error('older fixture failure'));
        await older;
        expect(state.repoErrors[configured.id]).toEqual([]);
        expect(state.repos[configured.id]).toEqual([{ id: 'admin-newer' }]);
    });
});

test('a completed forced ref refresh rejects an older successful response', async () => {
    await fixture(async (state, calls) => {
        const url = item('main').url;
        const older = state.ensureRefs([url]);
        const newer = state.ensureRefs([url], true);
        calls[1].resolve([{ url, branches: ['admin-newer'], tags: [] }]);
        await newer;
        calls[0].resolve([{ url, branches: ['admin-older'], tags: [] }]);
        await older;
        expect(state.refs[url].branches).toEqual(['admin-newer']);
    });
});

test('concurrent commit histories request each selected branch', async () => {
    await fixture(async (state, calls) => {
        const main = state.ensureCommits(item('main'));
        const feature = state.ensureCommits(item('feature'));
        for (const call of calls) call.resolve([{ sha: call.args.branch, message: 'admin history' }]);
        await Promise.all([main, feature]);
        expect(calls.map(call => [call.command, call.args.branch])).toEqual([
            ['get_commits', 'main'], ['get_commits', 'feature'],
        ]);
    });
});

test('a failed commit history can issue a successful retry', async () => {
    await fixture(async (state, calls) => {
        const first = state.ensureCommits(item('main'));
        calls[0].reject(new Error('retry fixture failure'));
        await first;
        const retry = state.ensureCommits(item('main'));
        for (const call of calls.slice(1)) call.resolve([{ sha: 'admin-retry', message: 'admin history' }]);
        await retry;
        expect(calls.map(call => call.command)).toEqual(['get_commits', 'get_commits']);
    });
});

test('credential invalidation rejects a pending obsolete source response', async () => {
    await fixture(async (state, calls, configured) => {
        const pending = state.loadRepos(configured, false);
        state.credentials.invalidate(configured.id, 1);
        calls[0].resolve({ repos: [{ id: 'admin-obsolete' }], errors: [] });
        await pending;
        expect(state.repos[configured.id]).toBeUndefined();
        expect(state.repoErrors[configured.id]).toBeUndefined();
    });
});

test('repeated URLs in one ref request make one backend lookup', async () => {
    await fixture(async (state, calls) => {
        const url = item('main').url;
        const pending = state.ensureRefs([url, url]);
        calls[0].resolve([{ url, branches: ['main'], tags: [] }]);
        await pending;
        expect(calls).toHaveLength(1);
        expect(calls[0].args.urls).toEqual([url]);
    });
});
