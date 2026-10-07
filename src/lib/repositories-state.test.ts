import './test-support/svelte-loader.js';
const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import type { LocalStatus, Repo, RepoSet, SetItem } from './api';
import type { RepositoriesHost } from './state/repositories.svelte';
import type { View } from './workspace';

const { Repositories } = await import('./state/repositories.svelte');

const repo = (n: number): Repo => ({ id: `s:o/r${n}`, source: 's', org: 'o', name: `r${n}`, description: '', url: `https://h/o/r${n}.git`, defaultBranch: 'main', pushedAt: '', archived: false });
const item = (n: number, on = false): SetItem => ({ id: `i${n}`, repoId: `s:o/r${n}`, url: `https://h/o/r${n}.git`, org: 'o', name: `r${n}`, ref: { type: 'branch', name: 'main' }, on });
const status = (path: string, patch: Partial<LocalStatus> = {}): LocalStatus => ({ path, exists: true, repo: true, branch: 'main', tag: null, sha: 'a', upstream: 'origin/main', ahead: 0, behind: 0, dirty: 0, error: null, ...patch });

function setup(options: { view?: View; sets?: RepoSet[]; local?: Record<string, LocalStatus>; repos?: Repo[] } = {}) {
  const calls: string[][] = [];
  const started: string[][] = [];
  const sets = options.sets ?? [{ id: 'a', name: 'A', items: Array.from({ length: 40 }, (_, n) => item(n, n % 2 === 0)) }];
  const local: Record<string, LocalStatus> = options.local ?? {};
  const host = {
    ws: { sets, stars: [], activeSet: sets[0].id }, sources: [], allRepos: options.repos ?? Array.from({ length: 60 }, (_, n) => repo(n)), local,
    view: options.view ?? { kind: 'repos' }, set: sets[0], temporary: { sets: [] },
    dest: (entry: SetItem) => `/dev/${entry.name}`, collisionKey: (path: string) => path,
    activeTabId: 'repos', tabs: [], openView: () => {}, activateTab: () => {}, closeTab: async () => {},
    addRepo: (r: Repo, _notify: boolean, set: RepoSet) => { const added = item(Number(r.name.slice(1))); set.items.push(added); return added; }, startClone: async (items: SetItem[]) => { started.push(items.map(entry => entry.id)); }, toast: () => {}, newSet: () => {},
    checkExists: async (paths: string[]) => { calls.push(paths); },
  } as unknown as RepositoriesHost;
  return { store: new Repositories(host), calls, sets, started };
}

const wait = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

describe('home selection', () => {
  test('starts empty even when every set row carries on: true', () => {
    const { store } = setup();
    expect(store.selected).toHaveLength(0);
  });

  test('one ticked row is the only selected row, however many set flags are on', () => {
    const { store } = setup();
    const row = store.shown[5].item;
    store.setOn(row, true);
    expect(store.selected.map(entry => entry.id)).toEqual([row.id]);
    expect(store.selected[0].on === row.on).toBe(true);
  });

  test('a ticked row that a filter hides is not selected until it shows again', () => {
    const { store } = setup();
    const row = store.shown[5].item;
    store.setOn(row, true);
    store.filter({ query: 'r33' });
    expect(store.selected).toHaveLength(0);
    store.filter({ query: '' });
    expect(store.selected).toHaveLength(1);
  });

  test('clearing empties the home selection and leaves set flags alone', () => {
    const { store, sets } = setup();
    store.setOn(store.shown[0].item, true);
    store.clearSelection();
    expect(store.selected).toHaveLength(0);
    expect(sets[0].items.filter(entry => entry.on)).toHaveLength(20);
  });

  test('a set tab keeps the per-set flags', () => {
    const { store } = setup({ view: { kind: 'set' } });
    store.setOn(store.entries[1].item, true);
    expect(store.selected).toHaveLength(21);
  });
});

describe('status reads', () => {
  test('home reads only the rows in view', async () => {
    const { store, calls } = setup();
    store.setVisible(store.shown.slice(0, 12).map(entry => entry.item));
    await wait(400);
    expect(calls.flat()).toHaveLength(12);
  });

  test('ticked rows are read too, known rows are not read again', async () => {
    const { store, calls } = setup({ local: { '/dev/r0': status('/dev/r0') } });
    store.setVisible(store.shown.slice(0, 3).map(entry => entry.item));
    store.setOn(store.shown[30].item, true);
    store.refreshStatus();
    await wait(400);
    expect(calls.flat().sort()).toEqual(['/dev/r1', '/dev/r2', '/dev/r30']);
  });

  test('a status chip reads every folder that a set holds, and no remote-only repository', async () => {
    const { store, calls } = setup();
    store.setVisible([]);
    store.filter({ chip: 'behind' });
    store.refreshStatus();
    await wait(400);
    expect(calls.flat()).toHaveLength(40);
  });

  test('chip counts are marked partial while some folder has no status', () => {
    const { store } = setup();
    expect(store.partial).toBe(true);
  });
});

describe('cloning a remote-only repository', () => {
  (globalThis as { document?: unknown }).document ??= { activeElement: null, body: {} };

  test('at home nothing is cloned or added until a set is chosen', async () => {
    const { store, started, sets } = setup();
    const remote = store.remoteItem(repo(50));
    await store.cloneItems([remote]);
    expect(store.askSet).toMatchObject({ clone: true });
    expect(started).toEqual([]);
    expect(sets[0].items).toHaveLength(40);
  });

  test('the chosen set receives the repository, then it is cloned', async () => {
    const { store, started, sets } = setup();
    const remote = store.remoteItem(repo(50));
    await store.cloneInto([remote], sets[0]);
    expect(sets[0].items).toHaveLength(41);
    expect(started).toEqual([['i50']]);
  });

  test('in a set tab the set in view is used without asking', async () => {
    const { store, started } = setup({ view: { kind: 'set' } });
    await store.cloneItems([store.remoteItem(repo(50))]);
    expect(store.askSet).toBeNull();
    expect(started).toEqual([['i50']]);
  });

  test('a remote row whose folder is already on disk is not cloned', async () => {
    const { store, started } = setup({ local: { '/dev/r50': status('/dev/r50') } });
    const remote = store.remoteItem(repo(50));
    expect(store.isUncloned(remote)).toBe(false);
    await store.cloneItems([remote]);
    expect(started).toEqual([]);
    expect(store.askSet).toBeNull();
  });
});
