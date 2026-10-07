import './test-support/svelte-loader.js';
import { beforeEach, expect, test } from 'bun:test';

const { TemporarySets } = await import('./state/temporary-sets.svelte.ts');
const { withoutTemporaryActive } = await import('./temporary-set.ts');
const { RunNotices } = await import('./state/run-notices.ts');
const { NotificationStore } = await import('./notifications.svelte.ts');

const repo = (path, name = path.split('/').pop()) => ({ path, name, kind: 'normal', branch: 'main', detached: false, parent: null });
let host, views, closed, toasts, scans, ids;
const transport = {
  drainRequests: async () => [],
  startScan: async () => { scans += 1; return scans; },
  cancelScan: async () => true,
  cancelAllScans: async () => 0,
  subscribe: async () => () => {},
};

beforeEach(() => {
  views = []; closed = []; toasts = []; scans = 0; ids = 0;
  host = {
    ws: { activeSet: 's1', sets: [{ id: 's1', name: 'Mine', items: [] }] },
    tabs: [], activeTabId: '',
    openView: (view, setId) => { views.push([view, setId]); host.tabs.push({ id: `${view.kind}:${setId}`, setId, view }); },
    closeTab: async id => { closed.push(id); host.tabs = host.tabs.filter(tab => tab.id !== id); },
    toast: (message, kind) => { toasts.push([message, kind]); },
    newId: () => `id-${++ids}`,
  };
});

async function openWithRepos(sets, paths) {
  const set = await sets.open('/home/u/code');
  sets.store.apply({ type: 'batch', batch: { id: 1, repos: paths.map(path => repo(path)) } });
  sets.store.apply({ type: 'done', done: { id: 1, summary: { repositories: paths.length, directories: 1, unreadable: 0, linksSkipped: 0, capped: null, cancelled: false } } });
  return set;
}

test('Opening a folder creates a temporary set with a tab and rows pinned to their paths', async () => {
  const sets = new TemporarySets(host, transport);
  const set = await openWithRepos(sets, ['/home/u/code/a', '/home/u/code/b']);
  expect(views).toEqual([[{ kind: 'set' }, set.id]]);
  expect(sets.find(set.id).items.map(item => item.path)).toEqual(['/home/u/code/a', '/home/u/code/b']);
  expect(sets.find(set.id).items.every(item => item.on === false && item.url === '')).toBe(true);
});

test('Saving converts the set into a normal set, moves its tabs and drops the temporary copy', async () => {
  const sets = new TemporarySets(host, transport);
  const set = await openWithRepos(sets, ['/home/u/code/a']);
  host.activeTabId = host.tabs[0].id;
  host.ws.activeSet = set.id;
  expect(sets.save(set.id)).toBe('id-1');
  expect(host.ws.sets.map(entry => entry.id)).toEqual(['s1', 'id-1']);
  expect(host.ws.sets[1]).toMatchObject({ name: 'code', items: [{ id: 'id-2', path: '/home/u/code/a', on: false }] });
  expect(sets.find(set.id)).toBeUndefined();
  expect(host.tabs[0]).toMatchObject({ setId: 'id-1', id: 'set:id-1' });
  expect(host.activeTabId).toBe('set:id-1');
  expect(host.ws.activeSet).toBe('id-1');
});

test('A set that is still scanning cannot be saved', async () => {
  const sets = new TemporarySets(host, transport);
  const set = await sets.open('/home/u/code');
  expect(sets.save(set.id)).toBeNull();
  expect(host.ws.sets).toHaveLength(1);
  expect(toasts[0][1]).toBe('warn');
});

test('Discarding closes the tabs, forgets the set and leaves settings untouched', async () => {
  const sets = new TemporarySets(host, transport);
  const set = await openWithRepos(sets, ['/home/u/code/a']);
  host.ws.activeSet = set.id;
  await sets.discard(set.id);
  expect(closed).toEqual([`set:${set.id}`]);
  expect(sets.find(set.id)).toBeUndefined();
  expect(host.ws.activeSet).toBe('s1');
  expect(host.ws.sets).toHaveLength(1);
});

test('A temporary set nobody has open any more is released', async () => {
  const sets = new TemporarySets(host, transport);
  const set = await openWithRepos(sets, ['/home/u/code/a']);
  sets.releaseIfUnused(set.id);
  expect(sets.find(set.id)).toBeDefined();
  host.tabs = [];
  sets.releaseIfUnused(set.id);
  expect(sets.find(set.id)).toBeUndefined();
});

test('A folder that is itself a repository opens that repository directly', async () => {
  const sets = new TemporarySets(host, transport);
  const set = await sets.open('/home/u/code/solo');
  sets.store.apply({ type: 'batch', batch: { id: 1, repos: [repo('/home/u/code/solo/vendor/x'), repo('/home/u/code/solo')] } });
  const live = sets.find(set.id);
  expect(live.isRepository).toBe(true);
  expect(live.items.map(item => item.path)).toEqual(['/home/u/code/solo']);
  expect(views.at(-1)).toEqual([{ kind: 'repo', repoId: 'local:/home/u/code/solo', section: 'overview' }, set.id]);
  expect(closed).toEqual([`set:${set.id}`]);
});

test('The scan-finished notice offers Open, which reveals the set', async () => {
  const actions = [];
  const sets = new TemporarySets({ ...host, toast: (message, kind, action) => { actions.push([message, action]); } }, transport);
  const set = await openWithRepos(sets, ['/home/u/code/a']);
  const [message, action] = actions.at(-1);
  expect(message).toContain('Found 1 repository');
  expect(action.label).toBe('Open');
  views.length = 0;
  action.run();
  expect(views).toEqual([[{ kind: 'set' }, set.id]]);
});

test('A Git run shows one loading notice that becomes the outcome with Retry for the failed repositories', () => {
  const store = new NotificationStore({ set: () => 0, clear: () => {} });
  const notices = new RunNotices(store);
  const retried = [];
  const id = notices.begin('pull', 3);
  expect(store.items).toMatchObject([{ id, kind: 'loading', msg: 'Pulling 3 repositories…' }]);
  notices.finish(id, { verb: 'pull', total: 3, failed: ['b'], firstError: 'no route', retry: items => retried.push(...items), viewActivity: () => {} });
  expect(store.items).toHaveLength(1);
  expect(store.items[0]).toMatchObject({ id, kind: 'error', msg: 'Pull failed for 1 of 3 repositories', detail: 'no route' });
  store.act(id, store.items[0].actions[0]);
  expect(retried).toEqual(['b']);
});

test('A clean run turns the loading notice into success', () => {
  const store = new NotificationStore({ set: () => 0, clear: () => {} });
  const notices = new RunNotices(store);
  const id = notices.begin('fetch', 1);
  notices.finish(id, { verb: 'fetch', total: 1, failed: [], retry: () => {}, viewActivity: () => {} });
  expect(store.items[0]).toMatchObject({ kind: 'success', msg: 'Fetched 1 repository' });
});

test('Saving keeps a repository page tab and moves it to the saved set', async () => {
  const sets = new TemporarySets(host, transport);
  const set = await openWithRepos(sets, ['/home/u/code/a']);
  host.tabs.push({ id: 'repo:local:/home/u/code/a', setId: set.id, view: { kind: 'repo', repoId: 'local:/home/u/code/a', section: 'history' } });
  sets.save(set.id);
  expect(host.tabs.find(tab => tab.view.kind === 'repo')).toMatchObject({ setId: 'id-1', id: 'repo:local:/home/u/code/a', view: { section: 'history' } });
});

test('Saving gives items new ids and retargets item tabs and the settings tab', async () => {
  const sets = new TemporarySets(host, transport);
  const set = await openWithRepos(sets, ['/home/u/code/a']);
  host.tabs.push({ id: `item:${set.id}:found:/home/u/code/a`, setId: set.id, view: { kind: 'item', itemId: 'found:/home/u/code/a' } });
  host.tabs.push({ id: 'settings', setId: set.id, view: { kind: 'settings' } });
  sets.save(set.id);
  expect(host.tabs.find(tab => tab.view.kind === 'item')).toMatchObject({ setId: 'id-1', id: 'item:id-1:id-2', view: { itemId: 'id-2' } });
  expect(host.tabs.find(tab => tab.view.kind === 'settings').setId).toBe('id-1');
});

test('Discarding or releasing a set leaves the settings tab alone and moves it to a saved set', async () => {
  const sets = new TemporarySets(host, transport);
  const set = await openWithRepos(sets, ['/home/u/code/a']);
  host.tabs.push({ id: 'settings', setId: set.id, view: { kind: 'settings' } });
  await sets.discard(set.id);
  expect(closed).toEqual([`set:${set.id}`]);
  expect(host.tabs.find(tab => tab.id === 'settings').setId).toBe('s1');
  const again = await openWithRepos(sets, ['/home/u/code/b']);
  host.tabs = [{ id: 'settings', setId: again.id, view: { kind: 'settings' } }];
  sets.releaseIfUnused(again.id);
  expect(sets.find(again.id)).toBeUndefined();
  expect(host.tabs[0].setId).toBe('s1');
});

test('Settings never record a temporary set as the active one', () => {
  const sets = [{ id: 's1' }];
  expect(withoutTemporaryActive({ workspace: { activeSet: 'temp-3', sets } }).workspace.activeSet).toBe('s1');
  const saved = { workspace: { activeSet: 's1', sets } };
  expect(withoutTemporaryActive(saved)).toBe(saved);
});
