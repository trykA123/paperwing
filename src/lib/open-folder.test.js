import './test-support/svelte-loader.js';
import { beforeEach, expect, test } from 'bun:test';

const { OpenFolderStore, describeScan, folderName } = await import('./open-folder.svelte.ts');

let notices, scans, handlers, queue, cancelAll;
const repo = (path, extra = {}) => ({ path, name: folderName(path), kind: 'normal', branch: 'main', detached: false, parent: null, ...extra });
const summary = (extra = {}) => ({ repositories: 0, directories: 1, unreadable: 0, linksSkipped: 0, capped: null, cancelled: false, ...extra });
const transport = {
    drainRequests: async () => queue.splice(0),
    startScan: async path => { scans.push(path); return scans.length; },
    cancelScan: async id => { scans.push(`cancel:${id}`); return true; },
    cancelAllScans: async () => { cancelAll += 1; return 0; },
    subscribe: async next => { handlers = next; return () => { handlers = null; }; },
};
const notify = (message, kind) => notices.push([message, kind]);
const make = () => new OpenFolderStore(transport, notify);

beforeEach(() => { notices = []; cancelAll = 0; scans = []; handlers = null; queue = []; });

test('A startup folder request starts one scan and creates a scanning temporary set', async () => {
    queue.push({ action: { kind: 'openFolder', path: '/home/u/code' }, ignored: [] });
    const store = make();
    await store.start();
    expect(scans).toEqual(['/home/u/code']);
    expect(store.sets).toHaveLength(1);
    expect(store.sets[0]).toMatchObject({ name: 'code', path: '/home/u/code', scanning: true, repos: [] });
});

test('Batches merge in arrival order and repeated repositories are ignored', async () => {
    const store = make();
    const set = await store.open('/r');
    store.apply({ type: 'batch', batch: { id: 1, repos: [repo('/r/a'), repo('/r/b')] } });
    store.apply({ type: 'batch', batch: { id: 1, repos: [repo('/r/b'), repo('/r/c')] } });
    expect(store.sets[0].repos.map(item => item.name)).toEqual(['a', 'b', 'c']);
    expect(set.id).toBe('temp-1');
});

test('Events that arrive before the scan id is known are replayed', async () => {
    let release;
    const slow = new OpenFolderStore({ ...transport, startScan: () => new Promise(resolve => { release = () => resolve(7); }) }, notify);
    await slow.start();
    const opening = slow.open('/r');
    handlers.onBatch({ id: 7, repos: [repo('/r/a')] });
    handlers.onDone({ id: 7, summary: summary({ repositories: 1 }) });
    release();
    await opening;
    expect(slow.sets[0].repos).toHaveLength(1);
    expect(slow.sets[0].scanning).toBe(false);
    expect(notices).toEqual([['Found 1 repository in r', 'info']]);
});

test('The final summary ends scanning and reports the count and a hit cap', async () => {
    const store = make();
    await store.open('/big');
    store.apply({ type: 'batch', batch: { id: 1, repos: [repo('/big/a'), repo('/big/b')] } });
    store.apply({ type: 'done', done: { id: 1, summary: summary({ repositories: 2, capped: 'repositories' }) } });
    expect(store.sets[0]).toMatchObject({ scanning: false, capped: 'repositories' });
    expect(notices).toEqual([['Found 2 repositories in big (scan stopped at its limit)', 'warn']]);
});

test('A folder that is itself a repository is announced as one', () => {
    expect(describeScan({ name: 'app', path: '/x/app', repos: [repo('/x/app')], capped: null })).toBe('app is a repository');
    expect(describeScan({ name: 'empty', path: '/x/empty', repos: [], capped: null })).toBe('No repositories found in empty');
});

test('Ignored arguments are reported and a compare request is held without opening a scan', async () => {
    queue.push({ action: { kind: 'compareFolders', left: '/a', right: '/b' }, ignored: [{ arg: 'x.txt', reason: 'Only folders can be opened' }] });
    const store = make();
    await store.start();
    expect(scans).toEqual([]);
    expect(store.compare).toEqual({ left: '/a', right: '/b' });
    expect(notices[0]).toEqual(['Ignored x.txt: Only folders can be opened', 'warn']);
});

test('A forwarded launch signal drains new requests while running', async () => {
    const store = make();
    await store.start();
    queue.push({ action: { kind: 'openFolder', path: '/second' }, ignored: [] });
    handlers.onSignal();
    await new Promise(resolve => setTimeout(resolve, 0));
    expect(scans).toEqual(['/second']);
});

test('Dismissing a scanning set cancels it and discards its late events', async () => {
    const store = make();
    await store.start();
    await store.open('/r');
    store.dismiss('temp-1');
    handlers.onDone({ id: 1, summary: summary({ cancelled: true }) });
    expect(scans).toContain('cancel:1');
    expect(store.sets).toHaveLength(0);
    expect(notices).toEqual([]);
});

test('Starting cancels scans left by a previous page load', async () => {
    await make().start();
    expect(cancelAll).toBe(1);
});

test('Events for scans this page never started are dropped, and early buffers are capped', async () => {
    const store = make();
    await store.start();
    handlers.onBatch({ id: 99, repos: [repo('/stray/a')] });
    expect(store.early.size).toBe(0);
    let release;
    const slow = new OpenFolderStore({ ...transport, startScan: () => new Promise(resolve => { release = () => resolve(1); }) }, notify);
    await slow.start();
    const opening = slow.open('/r');
    for (let id = 10; id < 30; id += 1) handlers.onBatch({ id, repos: [repo('/x/a')] });
    expect(slow.early.size).toBe(8);
    release();
    await opening;
    expect(slow.early.size).toBe(0);
});

test('A failed scan start is reported and leaves no set', async () => {
    const store = new OpenFolderStore({ ...transport, startScan: async () => { throw new Error('refused'); } }, notify);
    expect(await store.open('/nope')).toBeNull();
    expect(store.sets).toHaveLength(0);
    expect(notices[0][1]).toBe('error');
});
