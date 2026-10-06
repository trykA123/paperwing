import './test-support/svelte-loader.js';
import { emit } from '@tauri-apps/api/event';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { expect, test } from 'bun:test';
import { parse } from 'svelte/compiler';
import { render } from 'svelte/server';
import { refChoices } from './compare-refs';
import { compareRows, detailFile, pathMatches } from './compare-view';
import { copyHunk, decodeText, encodeText } from './editor';
import { fuzzy, rank, segments } from './fuzzy';
import { tabId } from './workspace';
import { windowsPlatform, supportedRoot } from './test-support/platform-fixture';
import layoutGoldens from './test-support/layout-vectors.json';
import './workspace.test.ts';

const { app } = await import('./state.svelte.ts');
const { commands, shortcut } = await import('./commands');
const { CompareState, SetCompareState } = await import('./compare.svelte.ts');
const { default: CompareReferencePicker } = await import('../components/CompareReferencePicker.svelte');
const { default: SetCompare } = await import('../components/SetCompare.svelte');

test('Compare pickers separate local and remote refs, show set coverage, and keep snapshots read-only', () => {
    const previous = app.trees;
    const ref = (name, sha = 'a'.repeat(40), symbolic = '') => ({ name, sha, symbolic, current: false });
    const tree = { branches: [ref('master')], tags: [ref('v1', 'b'.repeat(40))],
        remotes: [{ name: 'origin', urls: [], refs: [ref('origin/HEAD', 'a'.repeat(40), 'refs/remotes/origin/master'), ref('origin/feature'), ref('origin/master')] }], stashes: [], submodules: [] };
    try {
        app.trees = { first: { data: tree }, second: { data: { ...tree, remotes: [] } } };
        const names = (kind, paths = ['first']) => refChoices(kind, app.trees, paths).map(choice => choice.value);
        const html = (kind, readOnly = false, paths = ['first']) => render(CompareReferencePicker,
            { props: { reference: kind === 'head' ? { kind } : kind === 'commit' ? { kind, sha: '' } : { kind, name: '' }, paths, label: 'RIGHT', readOnly } }).body;
        expect(names('branch')).toEqual(['master']);
        expect(names('remoteBranch')).toEqual(['origin/feature', 'origin/master']);
        expect(names('tag')).toEqual(['v1']);
        expect(names('commit')).toContain('a'.repeat(40));
        expect(names('commit').filter(sha => sha === 'a'.repeat(40)).length).toBe(1);
        expect(refChoices('branch', app.trees, ['first', 'second', 'first'])[0].note).toBe('2/2');
        expect(refChoices('remoteBranch', app.trees, ['first', 'second']).find(choice => choice.value === 'origin/feature').note).toBe('1/2');
        expect(html('branch')).not.toContain('disabled');
        expect(html('head', true)).toContain('HEAD (current checkout)');
        expect(html('head')).toContain('Whatever is checked out now');
        app.trees.first = { error: 'Cannot read checkout' };
        expect(html('branch')).toContain('Refs unavailable');
        expect(html('branch')).toContain('Cannot read checkout');
    } finally { app.trees = previous; }
});

test('Fuzzy search matches fzf-style subsequences, ranks tight matches first and highlights hits', () => {
    expect(fuzzy('xyz', 'origin/main')).toBeNull();
    expect(fuzzy('', 'anything')).toEqual({ score: 0, positions: [] });
    expect(fuzzy('om', 'origin/main').positions).toEqual([0, 7]);
    const names = ['origin/feature-1614', 'origin/feature/search', 'release/2.4', 'main'];
    expect(rank('main', names, name => name).map(entry => entry.item)).toEqual(['main']);
    expect(rank('feat 16', names, name => name).map(entry => entry.item)).toEqual(['origin/feature-1614']);
    expect(rank('fs', names, name => name)[0].item).toBe('origin/feature/search');
    expect(rank('', names, name => name).length).toBe(4);
    expect(segments('main', [0, 1])).toEqual([{ text: 'ma', hit: true }, { text: 'in', hit: false }]);
});

test('Right-click snapshot comparison starts read-only while ordinary Compare retains the working tree', () => {
    const previous = { ws: app.ws, tabs: app.tabs, active: app.activeTabId, comparisons: app.comparisons, trees: app.trees, window: globalThis.window };
    globalThis.window = { crypto: globalThis.crypto };
    try {
        const item = { id: 'compare-item', name: 'repo', ref: { type: 'branch', name: 'master' } };
        app.ws = { ...app.ws, root: 'C:\\test', activeSet: 'compare-owner', sets: [{ id: 'compare-owner', name: 'Test', items: [item] }] };
        app.tabs = []; app.comparisons = {}; app.trees = {};
        app.openCompare(item, true);
        expect(app.view.readOnly).toBe(true);
        expect(app.view.left.reference.kind).toBe('head');
        expect(app.view.right.reference.kind).toBe('head');
        expect(app.tabTitle(app.activeTab)).toBe('Compare (read-only)');
        app.openCompare(item);
        expect(app.view.right.reference.kind).toBe('workingTree');
        const tree = { branches: [], tags: [], remotes: [{ name: 'origin', urls: [], refs: [{ name: 'origin/feature', sha: 'a'.repeat(40), symbolic: '', current: false }] }], stashes: [], submodules: [] };
        app.trees[app.dest(item)] = { data: tree };
        const comparison = new SetCompareState('compare-owner', []);
        comparison.right = { kind: 'remoteBranch', name: 'origin/feature' };
        const html = render(SetCompare, { props: { comparison } }).body;
        expect(html).toContain('aria-label="Common right reference"');
        expect(html).toContain('value="origin/feature"');
    } finally {
        app.ws = previous.ws; app.tabs = previous.tabs; app.activeTabId = previous.active; app.comparisons = previous.comparisons; app.trees = previous.trees;
        if (previous.window === undefined) delete globalThis.window; else globalThis.window = previous.window;
    }
});

test('Tab shortcuts wrap in visible order, preserve set context, and respect close guards', async () => {
    const previous = { ws: app.ws, tabs: app.tabs, active: app.activeTabId, guards: app.bufferGuards };
    const event = { ctrlKey: true, metaKey: false, altKey: false, shiftKey: false };
    expect(shortcut({ ...event, key: 'Tab' })).toBe('tab-next');
    expect(shortcut({ ...event, key: 'Tab', shiftKey: true })).toBe('tab-previous');
    expect(shortcut({ ...event, key: 'w' })).toBe('tab-close');
    expect(shortcut({ ...event, key: 'w', shiftKey: true })).toBeUndefined();
    expect(shortcut({ ...event, key: 'Tab', altKey: true })).toBeUndefined();
    try {
        app.ws = { ...app.ws, sets: [{ id: 'a', name: 'A', items: [] }, { id: 'b', name: 'B', items: [] }], activeSet: 'a' };
        app.tabs = [{ id: 'set:a', setId: 'a', view: { kind: 'set' }, query: '', page: 0, filter: '' },
            { id: 'set:b', setId: 'b', view: { kind: 'set' }, query: '', page: 0, filter: '' }];
        app.activeTabId = 'set:a'; app.bufferGuards = new Map();
        commands([]).find(command => command.id === 'tab-previous').run();
        expect(app.activeTabId).toBe('set:b'); expect(app.ws.activeSet).toBe('b');
        commands([]).find(command => command.id === 'tab-next').run();
        expect(app.activeTabId).toBe('set:a');
        app.bufferGuards.set('set:a', async () => false);
        await commands([]).find(command => command.id === 'tab-close').run();
        expect(app.tabs.length).toBe(2); expect(app.activeTabId).toBe('set:a');
        app.bufferGuards.set('set:a', async () => true);
        await commands([]).find(command => command.id === 'tab-close').run();
        expect(app.tabs.map(tab => tab.id)).toEqual(['set:b']); expect(app.ws.activeSet).toBe('b');
        app.bufferGuards.clear(); await commands([]).find(command => command.id === 'tab-close').run();
        expect(app.tabs.length).toBe(1); expect(app.view.kind).toBe('set');
    } finally { app.ws = previous.ws; app.tabs = previous.tabs; app.activeTabId = previous.active; app.bufferGuards = previous.guards; }
});

test('P8 includes unchecked duplicates, bounds concurrency, keeps failures honest and releases sessions', async () => {
    const previousWindow = globalThis.window;
    globalThis.window = { crypto: globalThis.crypto };
    let serial = 0, active = 0, peak = 0;
    const endpoints = new Map(), closed = [];
    const summary = { same: 1, different: 0, leftOnly: 0, rightOnly: 0, typeConflict: 0, unavailable: 0, total: 1 };
    mockIPC(async (command, args) => {
        if (command === 'comparison_open') { const id = `set-${++serial}`; endpoints.set(id, args); active++; peak = Math.max(peak, active); return { id, generation: 0 }; }
        if (command === 'comparison_refresh') {
            await Promise.resolve();
            const endpoint = endpoints.get(args.id);
            if (endpoint.left.itemId === 'missing' && endpoint.left.reference.kind === 'head') return { status: 'missingLeft', problem: { kind: 'missingLeft', message: 'Missing HEAD' } };
            return { status: 'ready', snapshot: { id: args.id, generation: 1, left: { endpoint: endpoint.left }, right: { endpoint: endpoint.right },
                raw: summary, display: summary, fileCount: 1, options: args.options, history: { available: false } } };
        }
        if (command === 'comparison_files') return [{ id: 'binary', path: 'blob.bin', left: { kind: 'file' }, right: { kind: 'file' }, displayStatus: 'different', displayLines: null }];
        if (command === 'comparison_close') { active--; closed.push(args.id); return true; }
        throw Error(command);
    });
    try {
        const comparison = new SetCompareState('owner', []);
        await comparison.run([{ id: 'first', name: 'same-repo', folder: 'copy-1', on: true },
            { id: 'second', name: 'same-repo', folder: 'copy-2', on: false }, { id: 'missing', name: 'missing', on: false }]);
        expect(comparison.rows.map(row => row.itemId)).toEqual(['first', 'second', 'missing']);
        expect(comparison.rows.map(row => row.folder)).toEqual(['copy-1', 'copy-2', 'missing']);
        expect(comparison.rows.map(row => row.state)).toEqual(['ready', 'ready', 'missingLeft']);
        expect(comparison.rows[0].added).toBeNull();
        expect(comparison.rows[1].left.setId).toBe('owner');
        expect(peak).toBeLessThanOrEqual(2); expect(active).toBe(0); expect(closed.length).toBe(4);
        expect(comparison.busy).toBe(false);
        comparison.rows.push({ itemId: 'queued', state: 'queued' });
        await comparison.cancel(); expect(comparison.rows.at(-1).state).toBe('cancelled');
    } finally { clearMocks(); globalThis.window = previousWindow; }
});

test('P8 drilldown preserves the whole-set no-exclusions policy', () => {
    const previous = { ws: app.ws, tabs: app.tabs, active: app.activeTabId, comparisons: app.comparisons };
    try {
        app.ws = { ...app.ws, sets: [{ id: 'policy-owner', name: 'Policy', items: [{ id: 'policy-item', name: 'repo' }] }], activeSet: 'policy-owner' };
        app.tabs = []; app.comparisons = {};
        const endpoint = { setId: 'policy-owner', itemId: 'policy-item', reference: { kind: 'head' } };
        app.openSetCompareRow({ state: 'ready', itemId: 'policy-item', left: endpoint, right: endpoint,
            snapshot: { options: { normalizeEol: true, ignoreWhitespace: false } } });
        const comparison = app.comparisons[app.view.comparisonId];
        expect(comparison.excludes).toBe('');
        const file = { id: 'changed', path: 'build/changed.txt', displayStatus: 'different', left: { kind: 'file' }, right: { kind: 'file' } };
        expect(compareRows([file], 'all', '', comparison.excludes, ['build']).map(row => row.id)).toContain('changed');
    } finally { app.ws = previous.ws; app.tabs = previous.tabs; app.activeTabId = previous.active; app.comparisons = previous.comparisons; }
});

test('P6 text saves preserve UTF-8 BOM, CRLF, LF, CR and trailing newlines', () => {
    for (const text of ['hello\nworld\n', '\uFEFFhello\r\nworld\r\n', 'hello\rworld\r', 'no newline', '']) {
        const bytes = Array.from(new TextEncoder().encode(text));
        const format = decodeText(bytes);
        expect(encodeText(format.text, format)).toEqual(bytes);
    }
    expect(() => decodeText([0, 1])).toThrow();
    expect(() => decodeText([255])).toThrow();
    const mixed = decodeText(new TextEncoder().encode('one\r\ntwo\n'));
    expect(mixed.editable).toBe(false);
    expect(() => encodeText(mixed.text, mixed)).toThrow();
});

test('P7 hunk mappings support replacement, insertion and deletion without stale indices', () => {
    expect(copyHunk('one\nchanged\nthree', 'one\nold\nthree', 2, 2, 2, 2)).toBe('one\nchanged\nthree');
    expect(copyHunk('one\ninserted\nthree', 'one\nthree', 2, 2, 1, 0)).toBe('one\ninserted\nthree');
    expect(copyHunk('one\nthree', 'one\nremoved\nthree', 1, 0, 2, 2)).toBe('one\nthree');
    expect(() => copyHunk('one', 'two', 3, 3, 1, 1)).toThrow();
});

test('P4 folder filters preserve matching ancestors, exclusions and collapsed paths', () => {
    const side = kind => ({ kind, size: 10, modifiedMs: null, source: 'commitBlob', reason: null });
    const file = (id, path, status, kind = 'file') => ({ id, path, displayStatus: status, left: side(kind), right: side(kind) });
    const files = [file('dir', 'src', 'different', 'directory'), file('same', 'src/a.c', 'same'),
        file('diff', 'src/b.c', 'different'), file('left', 'src/c.h', 'leftOnly'), file('right', 'only.txt', 'rightOnly'),
        file('excluded', 'src/temp.orig', 'different')];
    const ids = (filter, query = '', expanded = ['src']) => compareRows(files, filter, query, '*.orig', expanded).map(file => file.id);
    expect(ids('all')).toEqual(['dir', 'same', 'diff', 'left', 'right']);
    expect(ids('same')).toEqual(['dir', 'same']);
    expect(ids('differences')).toEqual(['dir', 'diff', 'left', 'right']);
    expect(ids('orphans')).toEqual(['dir', 'left', 'right']);
    expect(ids('all', '*.c')).toEqual(['dir', 'same', 'diff']);
    expect(ids('all', '', [])).toEqual(['dir', 'right']);
    expect(pathMatches('src/a.c', '**/*.c')).toBe(true);
    expect(pathMatches('src/build/cache.txt', 'build/')).toBe(true);
    expect(pathMatches('src/a.c', 'src/*.h')).toBe(false);
});

test('P4 paged files ignore a replaced snapshot and preserve all rows', async () => {
    const previousWindow = globalThis.window;
    globalThis.window = { crypto: globalThis.crypto };
    const comparison = new CompareState();
    const snapshot = { id: 'pages', generation: 1, fileCount: 513 };
    comparison.snapshot = snapshot;
    const calls = [];
    mockIPC((command, args) => {
        if (command === 'comparison_files') {
            calls.push(args.offset);
            return Array.from({ length: args.offset === 0 ? 512 : 1 }, (_, index) => ({ id: `file-${args.offset + index}`, path: `file-${args.offset + index}`, left: { kind: 'file' } }));
        }
        if (command === 'comparison_close') return true;
        throw Error(command);
    });
    try {
        await comparison.loadAllFiles();
        expect(calls).toEqual([0, 512]);
        expect(comparison.files.length).toBe(513);
        let resolve;
        mockIPC(() => new Promise(done => resolve = done));
        const loading = comparison.loadAllFiles();
        await comparison.close();
        resolve([]);
        await loading;
        expect(comparison.files).toEqual([]);
    } finally {
        clearMocks();
        if (previousWindow === undefined) delete globalThis.window; else globalThis.window = previousWindow;
    }
});

test('P4 mixed file/folder conflicts remain visible even without matching descendants', () => {
    for (const reverse of [false, true]) {
        const file = { id: 'mixed', path: 'conflict.txt', displayStatus: 'typeConflict', left: { kind: reverse ? 'directory' : 'file' }, right: { kind: reverse ? 'file' : 'directory' } };
        expect(compareRows([file], 'differences', '*.txt', '', []).map(row => row.id)).toEqual(['mixed']);
        expect(compareRows([file], 'all', '', '', []).map(row => row.id)).toEqual(['mixed']);
    }
});

test('P4 active file preview determines details independently of folder selection', () => {
    const files = [{ id: 'a' }, { id: 'b' }];
    const view = { kind: 'fileDiff', comparisonId: 'compare', fileId: 'a', path: 'a', sessionId: 'backend', generation: 1 };
    expect(detailFile(view, 'compare', files, 'b', { id: 'backend', generation: 1 }).id).toBe('a');
    expect(detailFile(view, 'compare', files, 'b', { id: 'backend', generation: 2 })).toBeUndefined();
    expect(detailFile({ kind: 'compare' }, 'compare', files, 'b', { id: 'backend', generation: 1 }).id).toBe('b');
});

test('P4 deleting compare owner set releases its session after endpoints move', async () => {
    const previous = { ws: app.ws, tabs: app.tabs, active: app.activeTabId, comparisons: app.comparisons, window: globalThis.window };
    const closed = [];
    globalThis.window = { crypto: globalThis.crypto };
    mockIPC((command, args) => { if (command === 'comparison_close') closed.push(args.id); return true; });
    try {
        const item = { id: 'item', name: 'repo', ref: { type: 'branch', name: 'main' } };
        app.ws = { ...app.ws, activeSet: 'owner', sets: [{ id: 'owner', name: 'Owner', items: [item] }, { id: 'other', name: 'Other', items: [{ ...item }] }] };
        app.tabs = []; app.comparisons = {};
        app.openCompare(app.set.items[0]);
        const view = app.view;
        app.comparisons[view.comparisonId].id = 'owned-session';
        view.left.setId = 'other'; view.right.setId = 'other';
        app.deleteSet('owner');
        expect(closed).toEqual(['owned-session']);
        expect(app.comparisons[view.comparisonId]).toBeUndefined();
        expect(app.tabs.every(tab => tab.view.kind !== 'compare')).toBe(true);
    } finally {
        app.ws = previous.ws; app.tabs = previous.tabs; app.activeTabId = previous.active; app.comparisons = previous.comparisons;
        clearMocks(); if (previous.window === undefined) delete globalThis.window; else globalThis.window = previous.window;
    }
});

test('P4 compare closing releases its session and dependent previews only', async () => {
    const previousWindow = globalThis.window;
    globalThis.window = { crypto: globalThis.crypto };
    const previous = { ws: app.ws, tabs: app.tabs, active: app.activeTabId, comparisons: app.comparisons };
    const closed = [];
    mockIPC((command, args) => { if (command === 'comparison_close') closed.push(args.id); return true; });
    try {
        app.tabs = []; app.activeTabId = ''; app.comparisons = {};
        app.openView({ kind: 'set' });
        app.openCompare(app.set.items[0]);
        if (!app.set.items.length) {
            app.ws = { ...app.ws, sets: [{ ...app.set, items: [{ id: 'preview-item', name: 'repo', on: true, ref: { type: 'branch', name: 'main' } }] }] };
            app.openCompare(app.set.items[0]);
        }
        const compareTab = app.activeTab;
        app.comparisons[compareTab.view.comparisonId].id = 'backend-session';
        const preview = { kind: 'fileDiff', comparisonId: compareTab.view.comparisonId, fileId: 'one', path: 'file.txt', sessionId: 'backend-session', generation: 1 };
        app.openView(preview);
        app.openView({ ...preview, generation: 2 });
        expect(app.activeTab.view.generation).toBe(2);
        app.closeTab(compareTab.id);
        expect(app.tabs.map(tab => tab.view.kind)).toEqual(['set']);
        expect(app.view.kind).toBe('set');
        expect(closed).toEqual(['backend-session']);
    } finally {
        app.ws = previous.ws; app.tabs = previous.tabs; app.activeTabId = previous.active; app.comparisons = previous.comparisons; clearMocks();
        if (previousWindow === undefined) delete globalThis.window; else globalThis.window = previousWindow;
    }
});

test('P3 custom layout shares unmatched/nested brace and sanitization goldens with Rust', () => {
    const previous = { ws: app.ws, sources: app.sources };
    const item = { id: 'item', repoId: 'source:repo', name: 'repo', folder: 'repo-folder', org: 'org', ref: { type: 'branch', name: 'feature/x' } };
    try {
        app.ws = { ...app.ws, root: 'C:\\fixture', layout: 'custom', activeSet: 'layout', sets: [{ id: 'layout', name: 'Set/Name', items: [item] }] };
        app.sources = [{ id: 'source', name: 'Source\\Name' }];
        for (const [template, expected] of layoutGoldens) {
            app.ws.pathTemplate = template;
            expect(app.segments(item)).toEqual(expected);
            expect(app.dest(item)).toBe(['C:\\fixture', ...expected].join('\\'));
        }
    } finally { app.ws = previous.ws; app.sources = previous.sources; }
});

function removalFixture() {
    const item = id => ({ id, repoId: id, url: id, org: 'org', name: id, on: true, ref: { type: 'branch', name: 'main' } });
    app.ws.sets = [{ id: 'a', name: 'A', items: [item('one'), item('two')] },
        { id: 'b', name: 'B', items: [item('one')] }];
    app.ws.activeSet = 'a';
    app.tabs = [];
    app.activeTabId = '';
    app.ready = true;
    app.openView({ kind: 'set' });
    app.openView({ kind: 'item', itemId: 'one' });
    return { id: 'one', name: 'one' };
}

for (const origin of ['favorite', 'org browser', 'search browser']) {
    test(`${origin} removal closes only the removed item tab`, () => {
        const repo = removalFixture();
        app.openView({ kind: 'item', itemId: 'one' }, 'b');
        app.openView(origin === 'org browser' ? { kind: 'org', source: 'src', org: 'org' }
            : origin === 'search browser' ? { kind: 'search' } : { kind: 'set' }, 'a');
        app.toggleRepo(repo);
        expect(app.tabs.some(tab => tab.id === 'item:a:one')).toBe(false);
        expect(app.tabs.some(tab => tab.id === 'item:b:one')).toBe(true);
        expect(app.set.id).toBe('a');
        expect(app.set.items.map(item => item.id)).toEqual(['two']);
        app.openView({ kind: 'item', itemId: 'one' });
        expect(app.actionItems).toEqual([]);
        expect(commands().filter(command => ['clone', 'fetch', 'pull', 'switch', 'status'].includes(command.id))
            .every(command => !command.enabled)).toBe(true);
        expect(commands().some(command => command.id === 'code')).toBe(false);
    });
}

test('duplicate removal preserves the surviving copy tab and its set context', () => {
    removalFixture();
    app.set.items.push({ ...app.set.items[0], id: 'copy', folder: 'one_2' });
    app.openView({ kind: 'item', itemId: 'copy' });
    app.removeItem('one');
    expect(app.tabs.some(tab => tab.id === 'item:a:one')).toBe(false);
    expect(app.activeTabId).toBe('item:a:copy');
    expect(app.focusedItem.id).toBe('copy');
    expect(app.actionItems.map(item => item.id)).toEqual(['copy']);
    app.removeItem('copy');
    expect(app.view.kind).toBe('set');
    expect(app.set.id).toBe('a');
});

test('palette navigation belongs only to input; arrows and Enter execute once', async () => {
    const source = await Bun.file(new URL('../components/CommandPalette.svelte', import.meta.url)).text();
    const ast = parse(source, { modern: true });
    const dialog = ast.fragment.nodes.find(node => node.name === 'dialog');
    const search = dialog.fragment.nodes.find(node => node.name === 'div');
    const input = search.fragment.nodes.find(node => node.name === 'input');
    expect(dialog.attributes.some(attribute => attribute.name === 'onkeydown')).toBe(false);
    expect(input.attributes.find(attribute => attribute.name === 'onkeydown').value.expression.name).toBe('key');
    expect(search.attributes.some(attribute => attribute.name === 'onkeydown')).toBe(false);
    const declaration = ast.instance.content.body.find(node => node.type === 'FunctionDeclaration' && node.id.name === 'key');
    const handler = new Bun.Transpiler({ loader: 'ts' }).transformSync(source.slice(declaration.start, declaration.end));
    const invoked = [], scrolled = [];
    const available = [{ id: 'first' }, { id: 'second' }];
    const makeHandler = new Function('available', 'current', 'execute', 'dialog',
        `let selected = current; ${handler}; return event => { current = Math.min(selected, Math.max(0, available.length - 1)); key(event); };`);
    let prevented = 0;
    const key = makeHandler(available, 0, command => invoked.push(command.id),
        { querySelectorAll: () => available.map(command => ({ scrollIntoView: () => scrolled.push(command.id) })) });
    for (const name of ['ArrowDown', 'ArrowUp', 'Enter']) key({ key: name, preventDefault: () => prevented++ });
    expect(scrolled).toEqual(['second', 'first']);
    expect(invoked).toEqual(['first']);
    expect(prevented).toBe(3);
    makeHandler([], 0, command => invoked.push(command.id), {})({ key: 'Enter', preventDefault() {} });
    expect(invoked).toEqual(['first']);
});

test('tabs use set/item IDs, and browse contexts cannot bleed between sets', () => {
    expect(tabId({ kind: 'set' }, 'a')).not.toBe(tabId({ kind: 'set' }, 'b'));
    expect(tabId({ kind: 'item', itemId: 'copy-1' }, 'a')).not.toBe(tabId({ kind: 'item', itemId: 'copy-2' }, 'a'));
    expect(tabId({ kind: 'org', source: 'src', org: 'org' }, 'a')).not.toBe(tabId({ kind: 'org', source: 'src', org: 'org' }, 'b'));
    expect(tabId({ kind: 'search' }, 'a')).not.toBe(tabId({ kind: 'search' }, 'b'));
});

async function treeFixture(run) {
    const previousWindow = globalThis.window;
    globalThis.window = { crypto: globalThis.crypto, setTimeout: () => 0 };
    const reads = [], statuses = [];
    mockIPC((command, args) => {
        if (command === 'load_settings') return { sources: [], workspace: null };
        if (command === 'platform_info') return windowsPlatform;
        if (command === 'probe_root') return supportedRoot(args.root);
        if (command === 'activity_snapshot') return [];
        if (command === 'repository_tree') return new Promise((resolve, reject) => reads.push({ path: args.path, resolve, reject }));
        if (command === 'local_status') return new Promise(resolve => statuses.push({ paths: args.paths, resolve }));
        throw new Error(`Unexpected IPC: ${command}`);
    }, { shouldMockEvents: true });
    try {
        app.tabs = [];
        app.trees = {};
        app.openTreePaths = [];
        await app.init();
        app.ws.root = 'C:\\fixture';
        app.set.items = [{ id: 'tree', repoId: 'tree', url: 'https://invalid.test/repo', org: 'org', name: 'repo', on: true, ref: { type: 'branch', name: 'main' } }];
        const data = name => ({ branches: [{ name, sha: name, current: true, symbolic: '' }], remotes: [], tags: [], stashes: [], submodules: [] });
        await run({ path: app.dest(app.set.items[0]), reads, statuses, data });
    } finally {
        app.openTreePaths = [];
        clearMocks();
        if (previousWindow === undefined) delete globalThis.window;
        else globalThis.window = previousWindow;
    }
}

test('P2 operation completion discards pending collapsed trees and reopen fetches again', async () => {
    await treeFixture(async ({ path, reads, statuses, data }) => {
        app.openTreePaths = [path];
        const pending = app.loadTree(path);
        app.openTreePaths = [];
        await emit('clone-finished');
        reads[0].resolve(data('obsolete'));
        await pending;
        expect(app.trees[path]).toBeUndefined();
        statuses[0].resolve([]);
        await new Promise(resolve => setImmediate(resolve));
        app.openTreePaths = [path];
        const reopened = app.loadTree(path);
        expect(reads.map(read => read.path)).toEqual([path, path]);
        reads[1].resolve(data('current'));
        await reopened;
        expect(app.trees[path].data).toEqual(data('current'));
    });
});

test('P2 focus/status refresh invalidates only its roots before pending status resolves', async () => {
    await treeFixture(async ({ path, reads, statuses, data }) => {
        const otherPath = 'D:\\other-root';
        const pending = app.loadTree(path);
        const other = app.loadTree(otherPath);
        reads[1].resolve(data('unaffected'));
        await other;
        const refreshing = app.checkExists([path]);
        reads[0].resolve(data('obsolete'));
        await pending;
        expect(app.trees[path]).toBeUndefined();
        expect(app.trees[otherPath].data).toEqual(data('unaffected'));
        statuses[0].resolve([]);
        await refreshing;
        const reopened = app.loadTree(path);
        expect(reads).toHaveLength(3);
        reads[2].resolve(data('current'));
        await reopened;
        expect(app.trees[path].data).toEqual(data('current'));
    });
});

test('P2 overlapping explicit refreshes ignore obsolete data and errors', async () => {
    await treeFixture(async ({ path, reads, data }) => {
        const first = app.loadTree(path);
        const second = app.loadTree(path, true);
        const latest = app.loadTree(path, true);
        reads[2].resolve(data('latest'));
        await latest;
        reads[0].resolve(data('obsolete'));
        reads[1].reject(new Error('obsolete error'));
        await Promise.all([first, second]);
        expect(app.trees[path]).toEqual({ data: data('latest') });
    });
});

async function compareFixture(run) {
    const previousWindow = globalThis.window;
    globalThis.window = { crypto: globalThis.crypto };
    const reads = [], closed = [], cancelled = [];
    mockIPC((command, args) => {
        if (command === 'comparison_close') { closed.push(args.id); return true; }
        if (command === 'comparison_cancel') { cancelled.push(args.id); return true; }
        return new Promise((resolve, reject) => reads.push({ command, args, resolve, reject }));
    });
    const endpoint = { setId: 'set', itemId: 'copy', reference: { kind: 'head' } };
    const result = generation => ({ status: 'ready', snapshot: { id: 'session', generation, options: { normalizeEol: false, ignoreWhitespace: false }, history: { available: false }, fileCount: 1 } });
    try { await run({ state: new CompareState(), reads, closed, cancelled, endpoint, result }); }
    finally {
        clearMocks();
        if (previousWindow === undefined) delete globalThis.window;
        else globalThis.window = previousWindow;
    }
}

test('P3 compare state ignores stale snapshots/content and uses opaque IDs', async () => {
    await compareFixture(async ({ state, reads, cancelled, endpoint, result }) => {
        const opening = state.open(endpoint, { ...endpoint, reference: { kind: 'workingTree' } });
        expect(reads[0].args.left).toEqual(endpoint);
        expect('path' in reads[0].args.left).toBe(false);
        reads[0].resolve({ id: 'session', generation: 0 });
        await new Promise(resolve => setImmediate(resolve));
        reads[1].resolve(result(1)); await opening;
        const old = state.refresh(), latest = state.refresh();
        reads[3].resolve(result(3)); await latest;
        reads[2].reject({ kind: 'cancelled', side: null, message: 'obsolete' }); await old;
        expect(state.snapshot.generation).toBe(3);
        expect(state.error).toBeNull();
        const first = state.loadContent('opaque-one', 'left'), second = state.loadContent('opaque-two', 'right');
        expect(reads[5].args).toEqual({ id: 'session', generation: 3, fileId: 'opaque-two', side: 'right' });
        reads[5].resolve({ generation: 3, bytes: [2] }); await second;
        reads[4].resolve({ generation: 3, bytes: [1] }); await first;
        expect(state.content.bytes).toEqual([2]);
        const pending = state.loadFiles(); await state.cancel();
        reads[6].resolve([{ id: 'obsolete-file' }]); await pending;
        expect(state.files).toEqual([]);
        expect(cancelled).toEqual(['session']);
        await state.loadCommits();
        expect(reads).toHaveLength(7);
    });
});

test('P3 overlapping opens close abandoned backend sessions and unavailable stays a result', async () => {
    await compareFixture(async ({ state, reads, closed, endpoint }) => {
        const abandoned = state.open(endpoint, endpoint), current = state.open(endpoint, endpoint);
        reads[0].resolve({ id: 'abandoned', generation: 0 }); await abandoned;
        expect(closed).toEqual(['abandoned']);
        reads[1].resolve({ id: 'session', generation: 0 });
        await new Promise(resolve => setImmediate(resolve));
        reads[2].resolve({ status: 'unavailable', problem: { kind: 'unavailable', side: 'left', message: 'notCloned' } });
        await current;
        expect(state.result.status).toBe('unavailable');
        expect(state.error).toBeNull();
        expect(state.busy).toBe(false);
        await state.close();
        expect(closed).toEqual(['abandoned', 'session']);
        expect(state.id).toBeNull();
    });
});
