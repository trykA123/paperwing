import './test-support/svelte-loader';
import { expect, test } from 'bun:test';
import { defaultWorkspace, migrateWorkspace } from './workspace';
import { collisionKey, destination, pathClashes, segments, uniqueFolder } from './workspace-paths';
import vectors from './test-support/platform-layout-vectors.json';
import { withIpc, deferred } from './test-support/ipc-fixture';
import { linuxPlatform, linuxCapabilities, supportedRoot, windowsPlatform } from './test-support/platform-fixture';
import { unavailableRoot } from './platform';
import { api } from './api';
import { parse } from 'svelte/compiler';

const { app } = await import('./state.svelte');
const { commands, execute, shortcut } = await import('./commands');

test('Linux initialization requires an explicit root and preserves foreign saved settings', () => {
    expect(new app.constructor().ws.root).toBe('');
    expect(defaultWorkspace('linux').root).toBe('');
    expect(defaultWorkspace('unsupported').root).toBe('');
    const saved = { root: 'D:\\kept', future: { untouched: true }, sets: [{ id: 'kept', name: 'Kept', items: [] }], activeSet: 'kept' };
    const migrated = migrateWorkspace(saved, 'linux');
    expect(migrated.root).toBe(saved.root);
    expect(migrated.future).toEqual(saved.future);
    expect(migrateWorkspace(JSON.parse(JSON.stringify(migrated)), 'linux')).toEqual(migrated);
    expect(migrateWorkspace(saved, 'windows').root).toBe(saved.root);
});

test('Platform-tagged destination vectors match native spelling and shared Rust expectations', () => {
    for (const vector of vectors) {
        const workspace = { ...defaultWorkspace(vector.platform), root: vector.root, layout: vector.layout, pathTemplate: vector.pathTemplate };
        const item = { folder: vector.folder, name: vector.repo, org: vector.org, repoId: 'source:repo', ref: { type: 'branch', name: vector.ref } };
        const sources = [{ id: 'source', name: vector.source }];
        expect(segments(workspace, sources, item, vector.set, vector.platform)).toEqual(vector.segments);
        expect(destination(workspace, sources, item, vector.set, vector.platform)).toBe(vector.destination);
    }
});

test('Collision candidates distinguish Linux case while physical identities determine existing aliases', () => {
    const upper = '/fixture/Folder', lower = '/fixture/folder';
    expect(pathClashes([upper, lower], 'linux').size).toBe(2);
    expect(uniqueFolder('Folder', [{ name: 'folder' }], 'linux')).toBe('Folder');
    expect(uniqueFolder('Folder', [{ name: 'folder' }], 'windows')).toBe('Folder_2');
    const physical = { [upper]: { identity: 'device:inode' }, [lower]: { identity: 'device:inode' } };
    expect(pathClashes([upper, lower], 'linux', physical).get('physical:device:inode')).toBe(2);
    expect(collisionKey(upper, 'linux')).toStartWith('candidate:');
    expect(collisionKey(upper, 'linux', physical)).toStartWith('physical:');
});

test('Native probe IPC retains exact command names, arguments and typed results', async () => {
    const calls = [];
    await withIpc((command, args) => {
        calls.push([command, args]);
        if (command === 'platform_info') return linuxPlatform;
        if (command === 'probe_root') return supportedRoot(args.root, linuxCapabilities);
        if (command === 'path_identities') return args.paths.map(path => ({ path, exists: false, identity: null, reason: null }));
        throw Error(command);
    }, async () => {
        expect(await api.platformInfo()).toEqual(linuxPlatform);
        expect((await api.probeRoot('/fixture')).casePolicy).toBe('unknown');
        expect((await api.pathIdentities(['/fixture/new']))[0].identity).toBeNull();
        expect(calls).toEqual([['platform_info', {}], ['probe_root', { root: '/fixture' }], ['path_identities', { paths: ['/fixture/new'] }]]);
    });
});

test('Invalid root selection preserves settings and explicit native reassignment retains sets', async () => {
    const previous = { ws: app.ws, platform: app.platform, probes: app.rootProbes };
    await withIpc((command, args) => {
        if (command !== 'probe_root') throw Error(command);
        return args.root === '/fixture' ? supportedRoot(args.root, linuxCapabilities) : unavailableRoot(args.root, 'Foreign root: choose a native folder.');
    }, async () => {
        window.setTimeout = () => 0;
        try {
            app.platform = linuxPlatform; app.ws = migrateWorkspace({ root: 'C:\\kept' }, 'linux');
            const sets = JSON.stringify(app.ws.sets);
            expect(await app.chooseRoot('C:\\foreign')).toBe(false);
            expect(app.ws.root).toBe('C:\\kept');
            expect(await app.chooseRoot('/fixture')).toBe(true);
            expect(app.ws.root).toBe('/fixture');
            expect(JSON.stringify(app.ws.sets)).toBe(sets);
        } finally { app.ws = previous.ws; app.platform = previous.platform; app.rootProbes = previous.probes; }
    });
});

test('Stale physical-identity responses cannot overwrite the latest destination observations', async () => {
    const older = deferred(), newer = deferred();
    const previous = app.pathIdentities;
    await withIpc((_command, args) => args.paths[0] === '/older' ? older.promise : newer.promise, async () => {
        try {
            const first = app.refreshPathIdentities(['/older']), second = app.refreshPathIdentities(['/newer']);
            newer.resolve([{ path: '/newer', identity: 'new', exists: true, reason: null }]); await second;
            older.resolve([{ path: '/older', identity: 'old', exists: true, reason: null }]); await first;
            expect(Object.keys(app.pathIdentities)).toEqual(['/newer']);
        } finally { app.pathIdentities = previous; }
    });
});

test('Linux write commands stay disabled without root support despite stale editor eligibility', async () => {
    const previous = { platform: app.platform, editors: app.editorActions, copy: app.copyActions, active: app.activeTabId, ready: app.ready, recovery: app.recoveryOpen, probes: app.rootProbes };
    let invoked = 0;
    try {
        app.platform = linuxPlatform; app.rootProbes = {}; app.ready = true; app.activeTabId = 'fixture'; app.recoveryOpen = false;
        app.editorActions = { fixture: { canSave: true, canSaveLeft: true, canSaveRight: true, canCopyLeft: true, canCopyRight: true, canUndo: true, save: () => invoked++, copy: () => invoked++, undo: () => invoked++ } };
        app.copyActions = { fixture: { left: true, right: true, copy: () => invoked++ } };
        const unavailable = ['editor-save', 'editor-save-left', 'editor-save-right', 'hunk-left', 'hunk-right', 'copy-left', 'copy-right'];
        for (const id of unavailable) {
            const command = commands([]).find(command => command.id === id);
            expect(command.enabled).toBe(false); expect(command.reason).toBeTruthy(); execute(command);
        }
        expect(commands([]).find(command => command.id === 'recovery').enabled).toBe(true);
        const id = shortcut({ key: 's', ctrlKey: true, metaKey: false, altKey: false, shiftKey: false });
        execute(commands([]).find(command => command.id === id));
        expect(invoked).toBe(0); expect(app.recoveryOpen).toBe(false);
    } finally {
        app.platform = previous.platform; app.editorActions = previous.editors; app.copyActions = previous.copy;
        app.activeTabId = previous.active; app.ready = previous.ready; app.recoveryOpen = previous.recovery; app.rootProbes = previous.probes;
    }
});

test('A direct unsupported copy request cannot acquire a preview or mutate request state', async () => {
    const previous = { platform: app.platform, comparisons: app.comparisons, request: app.copyRequest };
    await withIpc(command => { throw Error(`Unexpected write request: ${command}`); }, async () => {
        window.setTimeout = () => 0;
        try {
            app.platform = linuxPlatform; app.copyRequest = null;
            app.comparisons = { fixture: { snapshot: { id: 'native', generation: 1, left: { endpoint: { reference: { kind: 'workingTree' } } } } } };
            await app.requestCopy('fixture', 'file', 'left');
            expect(app.copyRequest).toBeNull();
        } finally { app.platform = previous.platform; app.comparisons = previous.comparisons; app.copyRequest = previous.request; }
    });
});

test('Capabilities remain unavailable for a root without a native probe even on Windows', () => {
    const previous = { platform: app.platform, ws: app.ws, probes: app.rootProbes };
    try {
        app.platform = windowsPlatform; app.ws = { ...app.ws, root: 'C:\\unprobed' }; app.rootProbes = {};
        expect(app.capability('edit').supported).toBe(false);
        expect(app.capability('copy').supported).toBe(false);
    } finally { app.platform = previous.platform; app.ws = previous.ws; app.rootProbes = previous.probes; }
});

test('An older successful root probe cannot replace or grant eligibility after a newer refusal', async () => {
    const previous = { platform: app.platform, ws: app.ws, probes: app.rootProbes };
    const older = deferred(), newer = deferred(); let count = 0;
    await withIpc(() => ++count === 1 ? older.promise : newer.promise, async () => {
        try {
            app.platform = windowsPlatform; app.ws = { ...app.ws, root: 'C:\\overlap' }; app.rootProbes = {};
            const first = app.probeRoot(), second = app.probeRoot();
            expect(app.capability('edit').supported).toBe(false);
            newer.resolve(unavailableRoot(app.ws.root, 'Root was replaced.')); await second;
            older.resolve(supportedRoot(app.ws.root));
            expect((await first).valid).toBe(false);
            expect(app.capability('edit').supported).toBe(false);
            expect(app.rootSupport.reason).toBe('Root was replaced.');
        } finally { app.platform = previous.platform; app.ws = previous.ws; app.rootProbes = previous.probes; }
    });
});

test('Clone deduplication uses its captured physical observations during a newer background refresh', async () => {
    const previous = { platform: app.platform, ws: app.ws, probes: app.rootProbes, identities: app.pathIdentities, running: app.running };
    const pending = deferred(), waiting = deferred(); let submitted = 0;
    await withIpc((command, args) => {
        if (command === 'probe_root') return supportedRoot(args.root, linuxCapabilities);
        if (command === 'path_identities') {
            if (args.paths[0] === '/unrelated') return [{ path: '/unrelated', identity: 'unrelated', exists: true, reason: null }];
            waiting.resolve(args.paths); return pending.promise;
        }
        if (command === 'start_clone') { submitted++; return; }
        if (command === 'save_settings') return;
        throw Error(command);
    }, async () => {
        window.setTimeout = () => 0;
        try {
            app.platform = linuxPlatform; app.running = false;
            app.ws = defaultWorkspace('linux'); app.ws.root = '/fixture';
            const items = ['Folder', 'folder'].map((name, index) => ({ id: String(index), name, url: 'https://invalid.test/repo', ref: { type: 'branch', name: 'main' } }));
            app.set.items = items;
            const cloning = app.startClone(items), paths = await waiting.promise;
            await app.refreshPathIdentities(['/unrelated']);
            pending.resolve(paths.map(path => ({ path, identity: 'shared-physical-folder', exists: true, reason: null })));
            await cloning;
            expect(submitted).toBe(0); expect(app.running).toBe(false);
        } finally {
            app.platform = previous.platform; app.ws = previous.ws; app.rootProbes = previous.probes;
            app.pathIdentities = previous.identities; app.running = previous.running;
        }
    });
});

test('Clone admission rejects overlapping submission while native probes and submission are pending', async () => {
    const previous = { platform: app.platform, ws: app.ws, probes: app.rootProbes, running: app.running, preparing: app.clonePreparing };
    const probe = deferred(), started = deferred(), finish = deferred(); let probes = 0, submitted = 0;
    await withIpc((command, args) => {
        if (command === 'probe_root') { probes++; return probe.promise; }
        if (command === 'path_identities') return args.paths.map(path => ({ path, exists: true, identity: 'unique', reason: null }));
        if (command === 'save_settings') return;
        if (command === 'start_clone') { submitted++; started.resolve(); return finish.promise; }
        throw Error(command);
    }, async () => {
        window.setTimeout = () => 0;
        try {
            app.platform = linuxPlatform; app.running = false; app.clonePreparing = false;
            app.ws = defaultWorkspace('linux'); app.ws.root = '/fixture';
            const items = [{ id: 'clone', name: 'repo', url: 'https://invalid.test/repo', ref: { type: 'branch', name: 'main' } }];
            const first = app.startClone(items, 'switch');
            expect(app.clonePreparing).toBe(true);
            await app.startClone(items, 'switch'); expect(probes).toBe(1);
            probe.resolve(supportedRoot('/fixture', linuxCapabilities)); await started.promise;
            await app.startClone(items, 'switch'); expect(submitted).toBe(1); expect(app.running).toBe(true);
            finish.resolve(); await first;
            expect(app.clonePreparing).toBe(false); expect(app.running).toBe(true);
        } finally {
            app.platform = previous.platform; app.ws = previous.ws; app.rootProbes = previous.probes;
            app.running = previous.running; app.clonePreparing = previous.preparing;
        }
    });
});

test('Copy confirmation admits one operation before its asynchronous root probe', async () => {
    const source = await Bun.file(new URL('../components/CopyOperations.svelte', import.meta.url)).text();
    const ast = parse(source, { modern: true });
    const functions = ['apply', 'close'].map(name => ast.instance.content.body.find(node => node.type === 'FunctionDeclaration' && node.id.name === name));
    const probe = deferred(), native = deferred(); let probes = 0, applies = 0, cancellations = 0;
    const context = {
        platform: windowsPlatform, fileCapability: operation => windowsPlatform.capabilities[operation], comparisons: { fixture: { snapshot: { id: 'session', generation: 1, right: { endpoint: {} } }, refresh: async () => {}, loadAllFiles: async () => {} } },
        probeEndpoints: () => { probes++; return probe.promise; }, endpointCapability: () => ({ supported: true, reason: null }), copyRequest: {},
    };
    const calls = { copyApply: () => { applies++; return native.promise; }, copyCancel: async () => { cancellations++; return true; } };
    const factory = new Function('app', 'api', `let preview={id:'plan'},busy=false,error='',outcomes=null; const request={comparisonId:'fixture',id:'session',generation:1,side:'right'};
      ${functions.map(node => source.slice(node.start, node.end)).join('\n')}
      return {apply,close,get busy(){return busy;}};`);
    const operation = factory(context, calls);
    const first = operation.apply(); await operation.apply();
    expect(probes).toBe(1); expect(applies).toBe(0); expect(operation.busy).toBe(true);
    probe.resolve(); await Promise.resolve(); await Promise.resolve();
    expect(applies).toBe(1); expect(operation.busy).toBe(true);
    await operation.close(); expect(cancellations).toBe(1); expect(context.copyRequest).not.toBeNull();
    native.resolve([]); await first; expect(operation.busy).toBe(false);
});


test('Linux writes become eligible only after a successful native root probe', async () => {
    const previous = { platform: app.platform, ws: app.ws, probes: app.rootProbes };
    const writable = { ...linuxCapabilities, edit: { supported: true, reason: null }, copy: { supported: true, reason: null }, recovery: { supported: true, reason: null } };
    await withIpc((_command, args) => args.root === '/fixture' ? supportedRoot(args.root, writable) : supportedRoot(args.root, linuxCapabilities), async () => {
        try {
            app.platform = linuxPlatform; app.ws = { ...app.ws, root: '/fixture' }; app.rootProbes = {};
            expect(app.fileCapability('edit').supported).toBe(false);
            expect(app.fileCapability('copy').supported).toBe(false);
            expect(app.capability('recovery').supported).toBe(true);
            await app.probeRoot('/fixture');
            expect(app.fileCapability('edit').supported).toBe(true);
            expect(app.fileCapability('copy').supported).toBe(true);
            app.ws.root = '/unsupported'; await app.probeRoot('/unsupported');
            expect(app.capability('edit').supported).toBe(false);
            expect(app.fileCapability('edit').supported).toBe(true);
            expect(app.capability('readCompare').supported).toBe(true);
        } finally { app.platform = previous.platform; app.ws = previous.ws; app.rootProbes = previous.probes; }
    });
});


test('A supported comparison root stays writable inside an unsupported workspace root', () => {
    const previous = { platform: app.platform, ws: app.ws, probes: app.rootProbes };
    try {
        app.platform = linuxPlatform; app.ws = { ...app.ws, root: '/workspace' };
        app.rootProbes = { '/workspace': supportedRoot('/workspace', linuxCapabilities), '/workspace/repo': supportedRoot('/workspace/repo') };
        expect(app.capability('edit').supported).toBe(false);
        expect(app.capability('edit', '/workspace/repo').supported).toBe(true);
        expect(app.fileCapability('edit').supported).toBe(true);
        expect(app.fileCapability('copy').supported).toBe(true);
    } finally { app.platform = previous.platform; app.ws = previous.ws; app.rootProbes = previous.probes; }
});
