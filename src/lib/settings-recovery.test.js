import './test-support/svelte-loader.js';
import { expect, test } from 'bun:test';
import { deferred, withIpc } from './test-support/ipc-fixture.js';
import { linuxPlatform, supportedRoot } from './test-support/platform-fixture.js';
import { appLifecycle } from './test-support/app-lifecycle.js';

const { app } = await import('./state.svelte.ts');
const startupError = 'injected settings read failure';

function stateWithError() {
    const state = new app.constructor();
    state.ready = true;
    state.startupError = startupError;
    state.probeRoot = async () => supportedRoot(state.ws.root);
    return state;
}

async function recoveryFixture(load, run) {
    const calls = [];
    await withIpc((command, args) => {
        calls.push({ command, args });
        if (command === 'load_settings') return load();
        if (command === 'platform_info') return linuxPlatform;
        if (command === 'plugin:event|listen') return 1;
        if (command === 'activity_snapshot') return [];
        if (command === 'probe_root') return supportedRoot(args.root);
        if (command === 'source_revision') return 0;
        if (command === 'list_repos') return { repos: [], errors: [] };
        if (command === 'save_settings') return;
        throw Error(command);
    }, async () => {
        const state = new app.constructor();
        await state.init();
        await run(state, calls);
    });
}

test('a close request destroys the window without saving after a startup error', async () => {
    const saves = [];
    await withIpc(command => { saves.push(command); throw Error('save refused'); }, async () => {
        const state = stateWithError();
        state.init = async () => {};
        state.temporary.start = async () => {};
        const lifecycle = appLifecycle(state);
        const stop = lifecycle.mount();
        try {
            await lifecycle.close();
            expect(lifecycle.destroyed).toHaveLength(1);
            expect(saves).toEqual([]);
        } finally { stop(); }
    });
});

test('a close request still destroys the window when a normal save fails', async () => {
    await withIpc(() => { throw Error('disk full'); }, async () => {
        const state = stateWithError();
        state.startupError = null;
        state.init = async () => {};
        state.temporary.start = async () => {};
        const lifecycle = appLifecycle(state);
        const stop = lifecycle.mount();
        try {
            await lifecycle.close();
            expect(lifecycle.destroyed).toHaveLength(1);
            expect(state.notices.items.some(notice => notice.msg.includes('disk full'))).toBe(true);
        } finally { stop(); }
    });
});

test('a cancelled buffer guard keeps the window open during startup recovery', async () => {
    const state = stateWithError();
    state.init = async () => {};
    state.temporary.start = async () => {};
    state.guardBuffers = async () => false;
    const lifecycle = appLifecycle(state);
    const stop = lifecycle.mount();
    try {
        await lifecycle.close();
        expect(lifecycle.destroyed).toEqual([]);
    } finally { stop(); }
});

test('startup errors suppress autosave and its timer after workspace changes', async () => {
    const calls = [];
    await withIpc(command => { calls.push(command); }, async () => {
        const state = stateWithError();
        const lifecycle = appLifecycle(state);
        await lifecycle.autosave();
        state.ws.parallel = 2;
        await lifecycle.autosave();
        expect(calls.filter(command => command === 'save_settings')).toEqual([]);
        expect(lifecycle.timers.filter(timer => timer.ms === 400)).toEqual([]);
    });
});

test('Retry applies recovered settings, clears the startup error and resumes autosave', async () => {
    const retry = deferred();
    let loads = 0;
    await recoveryFixture(() => ++loads === 1 ? { sources: [], workspace: null, startupError } : retry.promise, async (state, calls) => {
        expect(state.startupError).toBe(startupError);
        const notice = state.notices.items.find(item => item.kind === 'error');
        const action = notice.actions.find(action => action.label === 'Retry');
        expect(action).toBeDefined();
        const reload = state.retrySettings.bind(state);
        let pending;
        state.retrySettings = () => { pending = reload(); return pending; };
        state.notices.act(notice.id, action);
        const recovered = { sources: [{ id: 'admin-source', name: 'admin', kind: 'manual', host: '', orgs: [], urls: [] }], workspace: { root: '/fixture/recovered', parallel: 3, theme: 'dark' } };
        retry.resolve(recovered);
        await pending;
        expect(state.startupError).toBeNull();
        expect(state.sources).toEqual(recovered.sources);
        expect(state.ws.root).toBe('/fixture/recovered');
        expect(state.ws.parallel).toBe(3);
        expect(state.ws.theme).toBe('dark');
        await appLifecycle(state).autosave();
        const saves = calls.filter(call => call.command === 'save_settings');
        expect(saves).toHaveLength(1);
        expect(saves[0].args.settings.workspace.root).toBe('/fixture/recovered');
        expect(state.notices.items.some(item => item.msg.includes('Could not load settings'))).toBe(false);
    });
});

test('a failed Retry replaces the startup notice and keeps saving paused', async () => {
    let loads = 0;
    await recoveryFixture(() => {
        if (++loads === 1) return { sources: [], workspace: null, startupError };
        throw Error('second load failed');
    }, async (state, calls) => {
        expect(state.retrySettings).toBeFunction();
        await state.retrySettings();
        expect(state.startupError).toContain('second load failed');
        const errors = state.notices.items.filter(item => item.kind === 'error');
        expect(errors).toHaveLength(1);
        expect(errors[0].msg).toContain('second load failed');
        expect(errors[0].actions[0].label).toBe('Retry');
        await appLifecycle(state).autosave();
        expect(calls.filter(call => call.command === 'save_settings')).toEqual([]);
    });
});

test('Retry returning defaults with a startup error preserves the current workspace and sources', async () => {
    let loads = 0;
    await recoveryFixture(() => ({ sources: [], workspace: null, startupError: ++loads === 1 ? startupError : 'still unreadable' }), async (state, calls) => {
        state.sources = [{ id: 'admin-local', name: 'admin', kind: 'manual', host: '', orgs: [], urls: [] }];
        state.ws.root = '/fixture/local';
        await state.retrySettings();
        expect(state.startupError).toBe('still unreadable');
        expect(state.sources[0].id).toBe('admin-local');
        expect(state.ws.root).toBe('/fixture/local');
        expect(state.notices.items.filter(item => item.kind === 'error')).toHaveLength(1);
        await appLifecycle(state).autosave();
        expect(calls.filter(call => call.command === 'save_settings')).toEqual([]);
    });
});

test('clone starts without a settings pre-save while startup recovery is pending', async () => {
    const calls = [];
    await withIpc((command, args) => {
        calls.push(command);
        if (command === 'probe_root') return supportedRoot(args.root);
        if (command === 'path_identities') return args.paths.map(path => ({ path, identity: path, exists: false, reason: null }));
        if (command === 'start_clone') return;
        throw Error(`unexpected ${command}`);
    }, async () => {
        const state = stateWithError();
        state.platform = linuxPlatform;
        state.ws.root = '/fixture/clone';
        await state.startClone([{ id: 'admin-item', repoId: 'admin-repo', url: 'https://fixture.invalid/admin/repo', folder: 'repo', ref: { type: 'branch', name: 'main' } }]);
        expect(calls).toContain('start_clone');
        expect(calls).not.toContain('save_settings');
    });
});
