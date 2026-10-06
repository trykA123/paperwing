import './test-support/svelte-loader.js';
import { expect, test } from 'bun:test';
import { deferred, withIpc } from './test-support/ipc-fixture.js';

const { app } = await import('./state.svelte.ts');

async function metadataFixture(run) {
    const calls = [];
    await withIpc((command, args) => {
        const response = deferred();
        calls.push({ command, args, ...response });
        return response.promise;
    }, () => run(new app.constructor(), calls));
}

test('Repository metadata discards older source responses after a forced refresh', async () => {
    await metadataFixture(async (state, calls) => {
        const source = { id: 'source', kind: 'github', name: 'Fixture' };
        state.sources = [source];
        const older = state.loadRepos(source, false), newer = state.loadRepos(source, true);
        expect(calls.map(call => [call.command, call.args.refresh])).toEqual([['list_repos', false], ['list_repos', true]]);
        calls[1].resolve({ repos: [{ id: 'newer' }], errors: ['newer warning'] });
        await newer;
        calls[0].resolve({ repos: [{ id: 'older' }], errors: ['older warning'] });
        await older;
        expect(state.repos.source).toEqual([{ id: 'newer' }]);
        expect(state.repoErrors.source).toEqual(['newer warning']);
        expect(state.loadingRepos.source).toBe(false);
    });
});

test('Repository metadata discards an older failure after a forced refresh', async () => {
    await metadataFixture(async (state, calls) => {
        const source = { id: 'source', kind: 'github', name: 'Fixture' };
        state.sources = [source];
        const older = state.loadRepos(source, false), newer = state.loadRepos(source, true);
        calls[1].resolve({ repos: [{ id: 'newer' }], errors: [] });
        await newer;
        calls[0].reject('older failure');
        await older;
        expect(state.repos.source).toEqual([{ id: 'newer' }]);
        expect(state.repoErrors.source).toEqual([]);
        expect(state.loadingRepos.source).toBe(false);
    });
});

test('Reference metadata rejects an older response after a forced refresh', async () => {
    await metadataFixture(async (state, calls) => {
        const url = 'https://fixture.invalid/repo';
        const older = state.ensureRefs([url, url]), newer = state.ensureRefs([url], true);
        expect(calls.map(call => call.args.urls)).toEqual([[url], [url]]);
        calls[1].resolve([{ url, branches: ['newer'], tags: [], branchShas: ['new-sha'], tagShas: [] }]);
        await newer;
        calls[0].resolve([{ url, branches: ['older'], tags: ['v1'], branchShas: ['old-sha'], tagShas: ['tag-sha'], error: 'older warning' }]);
        await older;
        expect(state.refs[url].branches).toEqual(['newer']);
    });
});

test('Commit histories use separate branch keys during and after loading', async () => {
    await metadataFixture(async (state, calls) => {
        state.sources = [{ id: 'source', kind: 'github', name: 'Fixture' }];
        state.repos = { source: [{ id: 'source:repo', source: 'source', org: 'org', name: 'repo', defaultBranch: 'main' }] };
        const item = { id: 'copy', repoId: 'source:repo', ref: { type: 'branch', name: 'main' } };
        const otherBranch = { ...item, ref: { type: 'branch', name: 'feature' } };
        const main = state.ensureCommits(item), feature = state.ensureCommits(otherBranch);
        expect(calls.map(call => [call.command, call.args.branch])).toEqual([['get_commits', 'main'], ['get_commits', 'feature']]);
        calls[0].resolve([{ sha: 'main-sha' }]);
        calls[1].resolve([{ sha: 'feature-sha' }]);
        await Promise.all([main, feature]);
        expect(state.commitsFor(item)).toEqual([{ sha: 'main-sha' }]);
        expect(state.commitsFor(otherBranch)).toEqual([{ sha: 'feature-sha' }]);
        await state.ensureCommits(otherBranch);
        expect(calls).toHaveLength(2);
    });
});

test('Activity clear watermark rejects stale snapshots while retained jobs can finish', async () => {
    await metadataFixture(async (state, calls) => {
        const entry = (serial, sequence = 1, activityState = 'succeeded') => ({ id: `git-${serial}`, sequence, startedAt: serial, state: activityState });
        state.mergeActivity(entry(1));
        state.mergeActivity(entry(2, 1, 'running'));
        state.mergeActivity(entry(3));
        const snapshot = state.refreshActivity(), clearing = state.clearActivity();
        state.mergeActivity(entry(4, 1, 'running'));
        expect(calls.map(call => call.command)).toEqual(['activity_snapshot', 'clear_activity']);
        calls[1].resolve({ through: 3, retained: ['git-2'], running: [entry(2, 2, 'running')] });
        await clearing;
        calls[0].resolve([entry(1, 2), entry(2, 1, 'running'), entry(3, 2)]);
        await snapshot;
        expect(state.activity).toEqual([entry(2, 2, 'running'), entry(4, 1, 'running')]);
        state.mergeActivity(entry(2, 3));
        state.mergeActivity(entry(2, 2, 'running'));
        expect(state.activity[0]).toEqual(entry(2, 3));
        const clearedAgain = state.clearActivity();
        calls[2].resolve({ through: 4, retained: [], running: [] });
        await clearedAgain;
        state.mergeActivity(entry(2, 4));
        state.mergeActivity(entry(4, 2));
        state.mergeActivity(entry(5));
        expect(state.activity).toEqual([entry(5)]);
    });
});

test('Captured ref labels stay separate from checkout action values', async () => {
    await metadataFixture(async (state, calls) => {
        const name = 'synthetic-secret-branch', url = '/fixture/repo';
        const item = { id: 'copy', repoId: 'source:repo', url, ref: { type: 'branch', name } };
        const loading = state.ensureRefs([url]);
        calls[0].resolve([{ url, branches: [name], tags: ['secret-tag'], branchLabels: ['[redacted]-branch'], tagLabels: ['[redacted]-tag'], branchShas: ['sha'], tagShas: ['tag-sha'], error: null }]);
        await loading;
        expect(state.refs[url].branches).toEqual([name]);
        expect(state.refState(item)).toBe('ok');
        expect(state.refLabel(item)).toBe('[redacted]-branch');
        expect(item.ref.name).toBe(name);
        const { refChoices } = await import('./compare-refs.ts');
        const tree = { branches: [{ name, label: '[redacted]-branch', sha: 'sha', current: true, symbolic: '' }], tags: [], remotes: [] };
        expect(refChoices('branch', { fixture: { data: tree } }, ['fixture'])).toEqual([{ value: name, label: '[redacted]-branch', note: '' }]);
    });
});

test('Selected checkout labels use captured local metadata before remote refs load', async () => {
    await metadataFixture(async (state) => {
        const name = 'synthetic-secret-branch';
        const item = { id: 'copy', repoId: 'source:repo', org: 'org', name: 'repo', url: '/fixture/repo', ref: { type: 'branch', name } };
        state.local[state.dest(item)] = { branch: name, branchLabel: '[redacted]-branch' };
        expect(state.refLabel(item)).toBe('[redacted]-branch');
        expect(item.ref.name).toBe(name);
        state.local = {};
        state.trees[state.dest(item)] = { data: { branches: [{ name, label: '[redacted]-branch' }], tags: [] } };
        expect(state.refLabel(item)).toBe('[redacted]-branch');
    });
});
