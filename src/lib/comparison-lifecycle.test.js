import './test-support/svelte-loader.js';
import { expect, test } from 'bun:test';
import { deferred, withIpc } from './test-support/ipc-fixture.js';

const { app } = await import('./state.svelte.ts');
const { CompareState, SetCompareState } = await import('./compare.svelte.ts');

const endpoint = { setId: 'owner', itemId: 'item', reference: { kind: 'head' } };
const summary = { same: 0, different: 1, leftOnly: 0, rightOnly: 0, typeConflict: 0, unavailable: 0, total: 1 };
const ready = (id, options = { normalizeEol: true, ignoreWhitespace: false }) => ({
    status: 'ready', snapshot: { id, generation: 1, options, raw: summary, display: summary, fileCount: 1, history: { available: false } },
});
const items = Array.from({ length: 4 }, (_, index) => ({ id: `item-${index}`, name: `repo-${index}`, on: true }));

test.each([
    ['open', ['comparison_close', 'comparison_open', 'comparison_refresh']],
    ['refresh', ['comparison_refresh']],
    ['cancel', ['comparison_cancel']],
    ['close', ['comparison_close']],
])('Dirty editor refusal preserves the comparison during %s until consent', async (action, expectedCommands) => {
    const calls = [];
    await withIpc((command, args) => {
        calls.push({ command, args });
        if (command === 'comparison_open') return { id: 'replacement', generation: 0 };
        if (command === 'comparison_refresh') return ready(args.id, args.options);
        if (command === 'comparison_cancel' || command === 'comparison_close') return true;
        throw Error(command);
    }, async () => {
        const state = new CompareState();
        state.id = 'session'; state.snapshot = ready('session').snapshot; state.result = ready('session');
        state.files = [{ id: 'file', path: 'file.txt' }]; state.content = { bytes: [1], generation: 1 };
        const before = { snapshot: state.snapshot, result: state.result, files: state.files, content: state.content };
        let accepted = false, guarded = 0;
        state.editorGuards.set('dirty', async () => { guarded++; return accepted; });
        const run = () => action === 'open' ? state.open(endpoint, endpoint) : state[action]();
        await run();
        expect(guarded).toBe(1);
        expect(calls).toEqual([]);
        expect(state.id).toBe('session');
        expect({ snapshot: state.snapshot, result: state.result, files: state.files, content: state.content }).toEqual(before);
        expect(state.busy).toBe(false);
        accepted = true;
        await run();
        expect(calls.map(call => call.command)).toEqual(expectedCommands);
        expect(state.id).toBe(action === 'close' ? null : action === 'open' ? 'replacement' : 'session');
    });
});

test('Closing a compare tab consults dependent dirty buffers and keeps unrelated tabs and sessions', async () => {
    const closed = [];
    await withIpc((command, args) => {
        if (command === 'comparison_close') { closed.push(args.id); return true; }
        throw Error(command);
    }, async () => {
        const state = new app.constructor();
        const item = { id: 'item', name: 'repo', ref: { type: 'branch', name: 'main' } };
        state.ws = { ...state.ws, activeSet: 'owner', sets: [{ id: 'owner', name: 'Owner', items: [item] }] };
        state.openView({ kind: 'set' });
        state.openCompare(item);
        const owner = state.activeTab;
        state.comparisons[owner.view.comparisonId].id = 'owned-session';
        const preview = fileId => ({ kind: 'fileDiff', comparisonId: owner.view.comparisonId, fileId, path: `${fileId}.txt`, sessionId: 'owned-session', generation: 1 });
        state.openView(preview('first'));
        const first = state.activeTabId;
        state.openView(preview('second'));
        const second = state.activeTabId;
        state.openCompare(item);
        const unrelated = state.activeTab;
        state.comparisons[unrelated.view.comparisonId].id = 'unrelated-session';
        const originalTabs = state.tabs.map(tab => tab.id), guards = [];
        let accepted = false;
        state.bufferGuards.set(owner.id, async () => { guards.push(owner.id); return true; });
        state.bufferGuards.set(first, async () => { guards.push(first); return accepted; });
        state.bufferGuards.set(second, async () => { guards.push(second); return true; });
        state.bufferGuards.set(unrelated.id, async () => { guards.push(unrelated.id); return false; });
        await state.closeTab(owner.id);
        expect(guards).toEqual([owner.id, first]);
        expect(state.tabs.map(tab => tab.id)).toEqual(originalTabs);
        expect(state.comparisons[owner.view.comparisonId].id).toBe('owned-session');
        expect(closed).toEqual([]);
        accepted = true; guards.length = 0;
        await state.closeTab(owner.id);
        expect(guards).toEqual([owner.id, first, second]);
        expect(state.tabs.map(tab => tab.id)).toEqual(['set:owner', unrelated.id]);
        expect(state.activeTabId).toBe(unrelated.id);
        expect(state.comparisons[owner.view.comparisonId]).toBeUndefined();
        expect(state.comparisons[unrelated.view.comparisonId].id).toBe('unrelated-session');
        expect(closed).toEqual(['owned-session']);
    });
});

test('Closing during comparison open releases the later backend session without refreshing it', async () => {
    const opened = deferred(), closed = [], calls = [];
    await withIpc((command, args) => {
        calls.push(command);
        if (command === 'comparison_open') return opened.promise;
        if (command === 'comparison_close') { closed.push(args.id); return true; }
        throw Error(command);
    }, async () => {
        const state = new CompareState();
        const opening = state.open(endpoint, endpoint);
        await state.close();
        opened.resolve({ id: 'late-session', generation: 0 });
        await opening;
        expect(calls).toEqual(['comparison_open', 'comparison_close']);
        expect(closed).toEqual(['late-session']);
        expect(state.id).toBeNull();
        expect(state.result).toBeNull();
        expect(state.snapshot).toBeNull();
        expect(state.busy).toBe(false);
    });
});

test('Whole-set cancellation releases both refreshing workers and leaves queued rows unopened', async () => {
    const started = deferred(), refreshes = [], opened = [], closed = [], cancelled = [], files = [];
    await withIpc((command, args) => {
        if (command === 'comparison_open') {
            const id = `session-${opened.length}`; opened.push({ id, ...args });
            return { id, generation: 0 };
        }
        if (command === 'comparison_refresh') {
            const response = deferred(); refreshes.push({ ...response, ...args });
            if (refreshes.length === 2) started.resolve();
            return response.promise;
        }
        if (command === 'comparison_cancel') { cancelled.push(args.id); return true; }
        if (command === 'comparison_close') { closed.push(args.id); return true; }
        if (command === 'comparison_files') { files.push(args.id); return []; }
        throw Error(command);
    }, async () => {
        const state = new SetCompareState('owner', []);
        const running = state.run(items);
        await started.promise;
        expect(state.rows.map(row => row.state)).toEqual(['comparing', 'comparing', 'queued', 'queued']);
        await state.cancel();
        expect(cancelled.sort()).toEqual(['session-0', 'session-1']);
        expect(closed.sort()).toEqual(['session-0', 'session-1']);
        refreshes[0].resolve(ready(refreshes[0].id));
        refreshes[1].reject('late failure');
        await running;
        expect(opened.map(open => open.left.itemId)).toEqual(['item-0', 'item-1']);
        expect(state.rows.map(row => [row.state, row.message, row.snapshot])).toEqual(items.map(() => ['cancelled', 'Cancelled', null]));
        expect(closed).toHaveLength(2);
        expect(files).toEqual([]);
        expect(state.busy).toBe(false);
    });
});

test('Whole-set cancellation releases both backend sessions that finish opening afterward', async () => {
    const started = deferred(), opens = [], closed = [], cancelled = [];
    await withIpc((command, args) => {
        if (command === 'comparison_open') {
            const response = deferred(); opens.push({ ...response, ...args });
            if (opens.length === 2) started.resolve();
            return response.promise;
        }
        if (command === 'comparison_close') { closed.push(args.id); return true; }
        if (command === 'comparison_cancel') { cancelled.push(args.id); return true; }
        throw Error(command);
    }, async () => {
        const state = new SetCompareState('owner', []);
        const running = state.run(items);
        await started.promise;
        await state.cancel();
        expect(closed).toEqual([]);
        opens[0].resolve({ id: 'late-0', generation: 0 });
        opens[1].resolve({ id: 'late-1', generation: 0 });
        await running;
        expect(closed.sort()).toEqual(['late-0', 'late-1']);
        expect(cancelled).toEqual([]);
        expect(opens.map(open => open.left.itemId)).toEqual(['item-0', 'item-1']);
        expect(state.rows.map(row => row.state)).toEqual(items.map(() => 'cancelled'));
        expect(state.busy).toBe(false);
    });
});

test('Whole-set comparisons retain captured refs and options across queued work and aggregate final rows', async () => {
    const started = deferred(), resume = deferred(), opens = [], refreshes = [], closed = [];
    await withIpc(async (command, args) => {
        if (command === 'comparison_open') {
            const id = `session-${opens.length}`; opens.push({ id, ...args });
            return { id, generation: 0 };
        }
        if (command === 'comparison_refresh') {
            refreshes.push(args);
            if (refreshes.length === 2) started.resolve();
            await resume.promise;
            const result = ready(args.id, args.options);
            result.snapshot.fileCount = 3;
            result.snapshot.raw = result.snapshot.display = { ...summary, same: 1, total: 2 };
            return result;
        }
        if (command === 'comparison_files') return [
            { id: 'changed', displayStatus: 'different', left: { kind: 'file' }, right: { kind: 'file' }, displayLines: { added: 3, removed: 2 } },
            { id: 'same', displayStatus: 'same', left: { kind: 'file' }, right: { kind: 'file' }, displayLines: { added: 50, removed: 50 } },
            { id: 'directory', displayStatus: 'different', left: { kind: 'directory' }, right: { kind: 'directory' }, displayLines: null },
        ];
        if (command === 'comparison_close') { closed.push(args.id); return true; }
        throw Error(command);
    }, async () => {
        const state = new SetCompareState('owner', []);
        state.left = { kind: 'branch', name: 'main' }; state.right = { kind: 'tag', name: 'v1' };
        state.options = { normalizeEol: false, ignoreWhitespace: true };
        const running = state.run(items);
        await started.promise;
        state.left.name = 'changed'; state.right.name = 'changed';
        state.options.normalizeEol = true; state.options.ignoreWhitespace = false;
        resume.resolve();
        await running;
        expect(opens.map(open => [open.left.reference, open.right.reference])).toEqual(items.map(() => [{ kind: 'branch', name: 'main' }, { kind: 'tag', name: 'v1' }]));
        expect(refreshes.map(refresh => refresh.options)).toEqual(items.map(() => ({ normalizeEol: false, ignoreWhitespace: true })));
        expect(state.rows.map(row => [row.itemId, row.state, row.added, row.removed, row.snapshot.options])).toEqual(items.map(item => [item.id, 'ready', 3, 2, { normalizeEol: false, ignoreWhitespace: true }]));
        expect(closed.sort()).toEqual(['session-0', 'session-1', 'session-2', 'session-3']);
        expect(state.busy).toBe(false);
    });
});
