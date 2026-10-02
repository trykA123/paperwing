import { listen } from '@tauri-apps/api/event';
import {
    api, type Activity,
    type Commit, type GitAction, type LocalStatus, type Phase, type Progress, type Ref, type Repo,
    type RepositoryTree,
    type SetItem, type Source, type Workspace,
} from './api';
import { CompareState, SetCompareState, type SetCompareRow } from './compare.svelte';
import { confirm } from './confirm';
import { DEFAULT_TEMPLATE, defaultWorkspace, migrateWorkspace, tabId, type ShellTab, type View } from './workspace';
export { DEFAULT_COLS, DEFAULT_TEMPLATE } from './workspace';
export type { View } from './workspace';

export type RefsEntry = {
  branches: string[]; tags: string[]; branchShas?: string[]; tagShas?: string[]; error?: string; loading?: boolean;
};
export type CommitsEntry = Commit[] | 'loading' | { error: string };
export type RefState = 'ok' | 'missing' | 'unknown' | 'unverified';

export const uid = () => Math.random().toString(36).slice(2, 10);
export const RUNNING: Phase[] = ['resolving', 'cloning', 'fetching', 'checkout'];
export const PHASE: Record<Phase, string> = {
  queued: 'Queued', resolving: 'Starting', cloning: 'Cloning', fetching: 'Fetching',
  checkout: 'Checkout', done: 'Done', failed: 'Failed', skipped: 'Skipped',
};
export type ToastKind = 'info' | 'success' | 'warn' | 'error';
export type ToastItem = { id: number; msg: string; kind: ToastKind; action?: { label: string; run: () => void } };
const TOAST_MS: Record<ToastKind, number> = { info: 4000, success: 4000, warn: 7000, error: 10000 };
export const refText = (r: Ref) => (r.type === 'commit' ? r.name.slice(0, 8) : r.name);
export const matches = (text: string, query: string) =>
  query.toLowerCase().split(/\s+/).filter(Boolean).every(w => text.toLowerCase().includes(w));
const isHex = (s: string) => /^[0-9a-f]{7,40}$/i.test(s);

export function ago(iso: string) {
  if (!iso) return '';
  const s = (Date.now() - Date.parse(iso)) / 1000;
  if (s < 3600) return `${Math.max(1, Math.round(s / 60))}m ago`;
  if (s < 86400) return `${Math.round(s / 3600)}h ago`;
  if (s < 86400 * 60) return `${Math.round(s / 86400)}d ago`;
  return new Date(iso).toLocaleDateString();
}

export const PATH_TOKENS = [
  { token: '{folder}', hint: 'folder name (custom name or repo name)' },
  { token: '{repo}', hint: 'repository name' },
  { token: '{org}', hint: 'organization' },
  { token: '{set}', hint: 'set name' },
  { token: '{ref}', hint: 'branch, tag or short SHA' },
  { token: '{source}', hint: 'source name, e.g. GitHub' },
];

class AppState {
  copyRequest = $state<{ comparisonId: string; id: string; generation: number; fileId: string; side: 'left' | 'right' } | null>(null);
  recoveryOpen = $state(false);
  gitDialog = $state<{ kind: 'commit' | 'branch'; path: string; name: string; itemId?: string; targets?: { path: string; name: string }[] } | null>(null);
  /** True while a push or branch deletion runs; clone jobs use `running` instead. */
  gitBusy = $state(false);
  copyActions = $state<Record<string, { left: boolean; right: boolean; copy: (side: 'left' | 'right') => void }>>({});
  requestCopy(comparisonId: string, fileId: string, side: 'left' | 'right') {
    const snapshot = this.comparisons[comparisonId]?.snapshot;
    if (!snapshot || this.copyRequest) return;
    this.copyRequest = { comparisonId, id: snapshot.id, generation: snapshot.generation, fileId, side };
  }
  async prepareDiskMutation() {
    if (!await this.guardBuffers()) return false;
    for (const tab of [...this.tabs]) if (tab.view.kind === 'fileDiff') await this.closeTab(tab.id);
    return !this.tabs.some(tab => tab.view.kind === 'fileDiff');
  }
  bufferGuards = new Map<string, () => Promise<boolean>>();
  editorActions = $state<Record<string, { save: (side?: number) => Promise<void>; dirty: boolean; next: (direction: number) => void;
    copy: (side: 'left' | 'right') => void; canCopyLeft: boolean; canCopyRight: boolean;
    canSave: boolean; canSaveLeft: boolean; canSaveRight: boolean; canNavigate: boolean; canUndo: boolean; undo: () => Promise<void> }>>({});
  async guardBuffers(ids?: Set<string>) {
    for (const [id, guard] of this.bufferGuards) if ((!ids || ids.has(id)) && !await guard()) return false;
    return true;
  }
  ready = $state(false);
  sources = $state<Source[]>([]);
  ws = $state<Workspace>(defaultWorkspace());
  repos = $state<Record<string, Repo[]>>({});
  repoErrors = $state<Record<string, string[]>>({});
  loadingRepos = $state<Record<string, boolean>>({});
  refs = $state<Record<string, RefsEntry>>({});
  commits = $state<Record<string, CommitsEntry>>({});
  exists = $state<Record<string, boolean>>({});
  local = $state<Record<string, LocalStatus>>({});
  jobs = $state<Record<string, Progress>>({});
  running = $state(false);
  activityOpen = $state(false);
  activity = $state<Activity[]>([]);
  trees = $state<Record<string, { data?: RepositoryTree; loading?: boolean; error?: string }>>({});
  openTreePaths: string[] = [];
  #treeGeneration = new Map<string, number>();
  #activityThrough = 0;
  #activityRetained = new Set<string>();
  tabs = $state<ShellTab[]>([]);
  activeTabId = $state('');
  comparisons = $state<Record<string, CompareState>>({});
  setComparisons = $state<Record<string, SetCompareState>>({});
  activeTab = $derived(this.tabs.find(tab => tab.id === this.activeTabId));
  view = $derived<View>(this.activeTab?.view ?? { kind: 'set' });
  focusedItem = $derived.by(() => {
    const view = this.view;
    return view.kind === 'item' ? this.set.items.find(item => item.id === view.itemId) : undefined;
  });
  paletteOpen = $state(false);
  get query() { return this.activeTab?.query ?? ''; }
  set query(value: string) { if (this.activeTab) this.activeTab.query = value; }
  toasts = $state<ToastItem[]>([]);
  pendingRename = $state(false);
  renameItemId = $state<string | null>(null);
  #toastTimers = new Map<number, number>();
  #toastSeq = 0;
  #runIds = $state<string[]>([]);
  #runMode: GitAction = 'clone';
  #cloneWaiters: (() => void)[] = [];

  allRepos = $derived(Object.values(this.repos).flat());
  repoById = $derived(new Map(this.allRepos.map(r => [r.id, r])));
  set = $derived(this.ws.sets.find(s => s.id === this.ws.activeSet) ?? this.ws.sets[0]);
  selected = $derived(this.set.items.filter(i => i.on));
  runProgress = $derived.by(() => {
    const jobs = this.#runIds.map(id => this.jobs[id]).filter(Boolean);
    const sum = jobs.reduce((n, j) => n + (RUNNING.includes(j.phase) ? j.pct : j.phase === 'queued' ? 0 : 100), 0);
    return {
      total: jobs.length,
      finished: jobs.filter(j => j.phase !== 'queued' && !RUNNING.includes(j.phase)).length,
      pct: jobs.length ? Math.round(sum / jobs.length) : 0,
    };
  });
  actionItems = $derived(this.view.kind === 'item' ? (this.focusedItem ? [this.focusedItem] : []) : this.selected);
  clashes = $derived.by(() => {
    const m = new Map<string, number>();
    for (const i of this.selected) {
      const d = this.dest(i).toLowerCase();
      m.set(d, (m.get(d) ?? 0) + 1);
    }
    return m;
  });

  async init() {
    const saved = await api.loadSettings();
    const ws = migrateWorkspace(saved.workspace);
    this.sources = saved.sources ?? [];
    this.ws = ws;
    this.openView({ kind: 'set' });
    await listen<Progress>('clone-progress', e => { this.jobs[e.payload.id] = e.payload; });
    await listen('clone-finished', () => this.#finished());
    await listen<Activity>('git-activity', event => this.mergeActivity(event.payload));
    await this.refreshActivity();
    this.ready = true;
    if (!this.sources.length) this.openView({ kind: 'settings' });
    await Promise.all(this.sources.map(s => this.loadRepos(s, false)));
  }

  toast(msg: string, kind: ToastKind = 'info', action?: ToastItem['action']) {
    const same = this.toasts.find(t => t.msg === msg && t.kind === kind);
    if (same) return this.#armToast(same);
    const item = { id: ++this.#toastSeq, msg, kind, action };
    this.toasts = [...this.toasts, item].slice(-4);
    this.#armToast(item);
  }

  #armToast(item: ToastItem) {
    clearTimeout(this.#toastTimers.get(item.id));
    this.#toastTimers.set(item.id, window.setTimeout(() => this.dismissToast(item.id), TOAST_MS[item.kind]));
  }

  dismissToast(id: number) {
    clearTimeout(this.#toastTimers.get(id));
    this.#toastTimers.delete(id);
    this.toasts = this.toasts.filter(t => t.id !== id);
  }

  holdToast(id: number) { clearTimeout(this.#toastTimers.get(id)); }

  releaseToast(id: number) {
    const item = this.toasts.find(t => t.id === id);
    if (item) this.#armToast(item);
  }

  mergeActivity(entry: Activity) {
    const serial = Number(entry.id.slice(4));
    if (serial <= this.#activityThrough && !this.#activityRetained.has(entry.id)) return;
    const current = this.activity.find(activity => activity.id === entry.id);
    if (current && current.sequence >= entry.sequence) return;
    this.activity = [...this.activity.filter(activity => activity.id !== entry.id), entry].sort((left, right) => left.startedAt - right.startedAt);
    while (this.activity.length > 64) {
      const index = this.activity.findIndex(activity => activity.state !== 'running');
      if (index < 0) break;
      this.activity.splice(index, 1);
    }
  }

  async refreshActivity() {
    for (const entry of await api.activitySnapshot()) this.mergeActivity(entry);
  }

  async clearActivity() {
    const cleared = await api.clearActivity();
    this.#activityThrough = cleared.through;
    this.#activityRetained = new Set(cleared.retained);
    this.activity = this.activity.filter(entry => Number(entry.id.slice(4)) > cleared.through || this.#activityRetained.has(entry.id));
    for (const entry of cleared.running) this.mergeActivity(entry);
  }

  async loadTree(path: string, force = false) {
    if (!force && this.trees[path]) return;
    const generation = (this.#treeGeneration.get(path) ?? 0) + 1;
    this.#treeGeneration.set(path, generation);
    this.trees[path] = { ...this.trees[path], loading: true, error: undefined };
    try {
      const data = await api.repositoryTree(path);
      if (this.#treeGeneration.get(path) === generation) this.trees[path] = { data };
    } catch (error) {
      if (this.#treeGeneration.get(path) === generation) this.trees[path] = { error: String(error) };
    }
  }

  #invalidateTrees(paths: string[]) {
    for (const path of paths) {
      this.#treeGeneration.set(path, (this.#treeGeneration.get(path) ?? 0) + 1);
      delete this.trees[path];
    }
  }

  async loadRepos(src: Source, refresh: boolean) {
    this.loadingRepos[src.id] = true;
    try {
      const list = await api.listRepos($state.snapshot(src) as Source, refresh);
      this.repos[src.id] = list.repos;
      this.repoErrors[src.id] = list.errors;
    } catch (e) {
      this.repoErrors[src.id] = [String(e)];
    } finally {
      this.loadingRepos[src.id] = false;
    }
  }

  openView(view: View, setId = this.ws.activeSet, query?: string) {
    const id = tabId(view, setId);
    let tab = this.tabs.find(tab => tab.id === id);
    if (!tab) {
      tab = { id, setId, view, query: query ?? '', page: 0, filter: '' };
      this.tabs.push(tab);
    } else {
      if (view.kind === 'fileDiff') tab.view = view;
      if (query !== undefined) tab.query = query;
    }
    this.activateTab(id);
  }

  activateTab(id: string) {
    const tab = this.tabs.find(tab => tab.id === id);
    if (!tab) return;
    if (this.ws.sets.some(set => set.id === tab.setId)) this.ws.activeSet = tab.setId;
    this.activeTabId = id;
  }

  cycleTab(direction: number) {
    if (!this.tabs.length) return;
    const index = this.tabs.findIndex(tab => tab.id === this.activeTabId);
    this.activateTab(this.tabs[(index + direction + this.tabs.length) % this.tabs.length].id);
  }

  async closeTab(id: string) {
    const target = this.tabs.find(tab => tab.id === id);
    const affected = new Set([id]);
    if (target?.view.kind === 'compare') {
      const comparisonId = target.view.comparisonId;
      for (const tab of this.tabs) if (tab.view.kind === 'fileDiff' && tab.view.comparisonId === comparisonId) affected.add(tab.id);
    }
    if (this.bufferGuards.size && !await this.guardBuffers(affected)) return;
    const index = this.tabs.findIndex(tab => tab.id === id);
    if (index < 0) return;
    const view = this.tabs[index].view;
    const closing = new Set([id]);
    if (view.kind === 'setCompare') {
      void this.setComparisons[view.comparisonId]?.cancel().catch(error => this.toast(String(error), 'error'));
      delete this.setComparisons[view.comparisonId];
    }
    if (view.kind === 'compare') {
      const state = this.comparisons[view.comparisonId];
      if (state) void state.close().catch(error => this.toast(String(error), 'error'));
      delete this.comparisons[view.comparisonId];
      for (const tab of this.tabs) if (tab.view.kind === 'fileDiff' && tab.view.comparisonId === view.comparisonId) closing.add(tab.id);
    }
    const activeRemoved = closing.has(this.activeTabId);
    this.tabs = this.tabs.filter(tab => !closing.has(tab.id));
    if (!activeRemoved) return;
    const next = this.tabs[Math.min(index, this.tabs.length - 1)];
    if (next) this.activateTab(next.id);
    else this.openView({ kind: 'set' });
  }

  tabTitle(tab: ShellTab) {
    const set = this.ws.sets.find(set => set.id === tab.setId);
    const view = tab.view;
    switch (view.kind) {
      case 'set': return set?.name ?? 'Set';
      case 'item': return set?.items.find(item => item.id === view.itemId)?.folder
        ?? set?.items.find(item => item.id === view.itemId)?.name ?? 'Repository';
      case 'org': return `${view.org} → ${set?.name ?? 'Set'}`;
      case 'search': return `Search → ${set?.name ?? 'Set'}`;
      case 'compare': return view.readOnly ? 'Compare (read-only)' : 'Compare';
      case 'setCompare': return `Compare ${set?.name ?? 'Set'}`;
      case 'fileDiff': return view.path.split('/').at(-1) ?? 'File diff';
      case 'settings': return 'Settings';
    }
  }

  openCompare(item: SetItem = this.actionItems[0], readOnly = false) {
    if (!item) return;
    const comparisonId = uid();
    this.comparisons[comparisonId] = new CompareState();
    this.openView({ kind: 'compare', comparisonId, readOnly,
      left: { setId: this.set.id, itemId: item.id, reference: { kind: 'head' } },
      right: { setId: this.set.id, itemId: item.id, reference: { kind: readOnly ? 'head' : 'workingTree' } } });
  }

  openSetCompare() {
    const comparisonId = uid();
    this.setComparisons[comparisonId] = new SetCompareState(this.set.id, this.set.items.map(item => ({ id: item.id, folder: this.folderOf(item), name: item.name })));
    this.openView({ kind: 'setCompare', comparisonId }, this.set.id);
  }
  openSetCompareRow(row: SetCompareRow) {
    const set = this.ws.sets.find(set => set.id === row.left.setId);
    if (!row.snapshot || row.state !== 'ready' || !set?.items.some(item => item.id === row.itemId)) return;
    const comparisonId = uid(); this.comparisons[comparisonId] = new CompareState();
    this.comparisons[comparisonId].options = { ...row.snapshot.options };
    this.comparisons[comparisonId].excludes = '';
    this.openView({ kind: 'compare', comparisonId, left: JSON.parse(JSON.stringify(row.left)), right: JSON.parse(JSON.stringify(row.right)) }, row.left.setId);
  }

  orgsOf(src: Source) {
    return src.kind === 'manual' ? [...new Set((this.repos[src.id] ?? []).map(r => r.org))] : src.orgs;
  }

  reposOf(sourceId: string, org: string) {
    const o = org.toLowerCase();
    return (this.repos[sourceId] ?? []).filter(r => r.org.toLowerCase() === o);
  }

  /** Folder path below the root, one entry per folder level. */
  segments(item: SetItem, setName = this.set.name): string[] {
    const folder = this.folderOf(item);
    if (this.ws.layout !== 'custom') return [folder];
    const flat = (s: string) => s.replace(/[\\/]+/g, '-');
    const source = this.sources.find(s => s.id === item.repoId.split(':')[0])?.name ?? '';
    const values: Record<string, string> = {
      folder, repo: item.name, org: item.org, set: flat(setName), source: flat(source),
      ref: flat(item.ref.type === 'commit' ? item.ref.name.slice(0, 8) : item.ref.name),
    };
    let tpl = this.ws.pathTemplate.trim() || DEFAULT_TEMPLATE;
    // Without a per-repo token every row would land in the same folder.
    if (!/\{(folder|repo)\}/.test(tpl)) tpl += '\\{folder}';
    return tpl
      .replace(/\{(\w+)\}/g, (m, k: string) => values[k] ?? m)
      .split(/[\\/]+/)
      .map(s => s.replace(/[:*?"<>|\x00-\x1f]/g, '').trim().replace(/[. ]+$/, ''))
      .filter(s => s && s !== '.' && s !== '..');
  }

  dest(item: SetItem, setId = this.set.id) {
    const sep = this.ws.root.includes('/') && !this.ws.root.includes('\\') ? '/' : '\\';
    const root = this.ws.root.replace(/[\\/]+$/, '');
    return [root, ...this.segments(item, this.ws.sets.find(set => set.id === setId)?.name ?? this.set.name)].join(sep);
  }

  folderOf(item: SetItem) {
    return item.folder || item.name;
  }

  hasClash(item: SetItem) {
    return (this.clashes.get(this.dest(item).toLowerCase()) ?? 0) > 1;
  }

  refState(item: SetItem): RefState {
    if (item.ref.type === 'commit') {
      const c = this.commits[item.repoId];
      if (Array.isArray(c) && c.some(x => x.sha.startsWith(item.ref.name))) return 'ok';
      return isHex(item.ref.name) ? 'unverified' : 'missing';
    }
    const r = this.refs[item.url];
    if (!r || r.loading || r.error) return 'unknown';
    return (item.ref.type === 'branch' ? r.branches : r.tags).includes(item.ref.name) ? 'ok' : 'missing';
  }

  async ensureRefs(urls: string[], force = false) {
    const need = [...new Set(urls)].filter(u => force || !this.refs[u] || this.refs[u].error);
    if (!need.length) return;
    for (const u of need) this.refs[u] = { branches: [], tags: [], loading: true };
    for (const r of await api.getRefsMany(need)) {
      this.refs[r.url] = { branches: r.branches, tags: r.tags, branchShas: r.branchShas, tagShas: r.tagShas, error: r.error ?? undefined };
    }
  }

  async ensureCommits(item: SetItem) {
    if (this.commits[item.repoId]) return;
    const repo = this.repoById.get(item.repoId);
    const src = this.sources.find(s => s.id === repo?.source);
    if (!repo || !src || src.kind === 'manual') { this.commits[item.repoId] = []; return; }
    this.commits[item.repoId] = 'loading';
    try {
      const branch = item.ref.type === 'branch' ? item.ref.name : repo.defaultBranch;
      this.commits[item.repoId] = await api.getCommits($state.snapshot(src) as Source, repo.org, repo.name, branch);
    } catch (e) {
      this.commits[item.repoId] = { error: String(e) };
    }
  }

  inSet(repoId: string) {
    return this.set.items.some(i => i.repoId === repoId);
  }

  countInSet(repoId: string) {
    return this.set.items.filter(i => i.repoId === repoId).length;
  }

  /** `base`, or `base_2`, `base_3`… — whichever is not yet used in the set. */
  uniqueFolder(base: string) {
    const taken = new Set(this.set.items.map(i => this.folderOf(i).toLowerCase()));
    if (!taken.has(base.toLowerCase())) return base;
    let n = 2;
    while (taken.has(`${base}_${n}`.toLowerCase())) n++;
    return `${base}_${n}`;
  }

  addRepo(repo: Repo, notify = true) {
    const id = uid();
    const folder = this.uniqueFolder(repo.name);
    this.set.items.push({
      id, repoId: repo.id, url: repo.url, org: repo.org, name: repo.name, on: true,
      ref: { type: 'branch', name: repo.defaultBranch || 'master' },
      ...(folder !== repo.name ? { folder } : {}),
    });
    if (notify) this.toast(folder === repo.name ? `${repo.name} added to ${this.set.name}` : `Another copy of ${repo.name} added as ${folder}`, 'success');
    // Manual sources have no API, so learn the default branch from the remote.
    if (!repo.defaultBranch) {
      this.ensureRefs([repo.url]).then(() => {
        const b = this.refs[repo.url]?.branches ?? [];
        const item = this.set.items.find(i => i.id === id);
        const best = ['main', 'master', 'develop'].find(n => b.includes(n)) ?? b[0];
        if (item && best) item.ref = { type: 'branch', name: best };
      });
    }
  }

  toggleRepo(repo: Repo) {
    const n = this.countInSet(repo.id);
    if (n === 0) this.addRepo(repo);
    else if (n === 1) this.removeItem(this.set.items.find(i => i.repoId === repo.id)!.id);
    else this.toast(`${repo.name} is in the set ${n} times; remove copies from the set table`, 'warn');
  }

  duplicateItem(id: string) {
    const items = this.set.items;
    const k = items.findIndex(i => i.id === id);
    if (k < 0) return;
    const copy = { ...($state.snapshot(items[k]) as SetItem), id: uid(), folder: this.uniqueFolder(this.folderOf(items[k])) };
    items.splice(k + 1, 0, copy);
    this.renameItemId = copy.id;
  }

  renameFolder(item: SetItem, value: string) {
    // Folder names are one path segment: no separators, reserved characters or leading dots.
    const v = value.replace(/[\\/:*?"<>|\x00-\x1f]/g, '').replace(/^[.\s]+|[.\s]+$/g, '');
    if (v && v !== item.name) item.folder = v;
    else delete item.folder;
  }

  toggleStar(id: string) {
    this.ws.stars = this.ws.stars.includes(id) ? this.ws.stars.filter(s => s !== id) : [...this.ws.stars, id];
  }

  setRef(item: SetItem, ref: Ref) {
    item.ref = ref;
    delete this.jobs[item.id];
  }

  async removeItem(id: string) {
    if (this.bufferGuards.size && !await this.guardBuffers()) return;
    const setId = this.set.id;
    for (const comparison of Object.values(this.setComparisons)) if (comparison.setId === setId && comparison.items.some(item => item.id === id)) { await comparison.cancel(); comparison.stale = true; }
    this.set.items = this.set.items.filter(i => i.id !== id);
    for (const tab of [...this.tabs]) {
      if (tab.setId === setId && tab.view.kind === 'item' && tab.view.itemId === id) this.closeTab(tab.id);
      if (tab.view.kind === 'compare' && [tab.view.left, tab.view.right].some(endpoint => endpoint.setId === setId && endpoint.itemId === id)) this.closeTab(tab.id);
    }
  }

  setAllOn(on: boolean) {
    for (const i of this.set.items) i.on = on;
  }

  newSet() {
    const id = uid();
    this.ws.sets.push({ id, name: `New set ${this.ws.sets.length + 1}`, items: [] });
    this.ws.activeSet = id;
    this.openView({ kind: 'set' }, id);
    this.pendingRename = true;
  }

  async deleteSet(id: string, trashFolders = false) {
    if (this.bufferGuards.size && !await this.guardBuffers()) return;
    if (trashFolders && (this.running || this.gitBusy)) { this.toast('Wait for the running Git operation to finish first', 'warn'); return; }
    for (const tab of [...this.tabs]) {
      if (tab.setId === id && tab.view.kind === 'setCompare') await this.closeTab(tab.id);
      if (tab.view.kind === 'compare' && (tab.setId === id || [tab.view.left, tab.view.right].some(endpoint => endpoint.setId === id))) await this.closeTab(tab.id);
    }
    if (trashFolders) {
      this.gitBusy = true;
      try {
        await api.saveSettings({ sources: this.sources, workspace: this.ws });
        const outcomes = await api.trashSetFolders(id);
        const moved = outcomes.filter(outcome => outcome.state === 'trashed').length;
        const kept = outcomes.filter(outcome => outcome.state === 'skipped' || outcome.state === 'failed');
        if (moved) this.toast(`Moved ${moved} folder${moved === 1 ? '' : 's'} to the Recycle Bin`, 'success');
        if (kept.length) this.toast(`${kept.length} folder${kept.length === 1 ? ' was' : 's were'} left in place. ${kept[0].reason ?? ''}`, 'warn');
      } catch (reason) {
        this.toast(String(reason), 'error');
        return;
      } finally { this.gitBusy = false; }
    }
    this.ws.sets = this.ws.sets.filter(s => s.id !== id);
    this.ws.activeSet = this.ws.sets[0].id;
    this.tabs = this.tabs.filter(tab => tab.setId !== id || tab.view.kind === 'settings');
    this.openView({ kind: 'set' });
  }

  openGitDialog(kind: 'commit' | 'branch', item: SetItem) {
    const path = this.dest(item);
    if (!this.local[path]?.repo) { this.toast('Clone the repository first', 'warn'); return; }
    if (this.gitDialog) return;
    this.gitDialog = { kind, path, name: this.folderOf(item), itemId: item.id };
  }

  openBranchDialog(items: SetItem[]) {
    const cloned = items.filter(item => this.local[this.dest(item)]?.repo);
    const targets = cloned.map(item => ({ path: this.dest(item), name: this.folderOf(item) }));
    if (!targets.length) { this.toast('Clone the repositories first', 'warn'); return; }
    if (this.gitDialog) return;
    this.gitDialog = { kind: 'branch', path: targets[0].path, name: targets[0].name, itemId: cloned.length === 1 ? cloned[0].id : undefined, targets };
  }

  /** Adds a row that clones the same repo into its own folder; the original row is left alone. */
  addCopy(item: SetItem, folder: string): SetItem {
    const items = this.set.items;
    const k = items.findIndex(i => i.id === item.id);
    items.splice(k + 1, 0, { ...($state.snapshot(item) as SetItem), id: uid(), folder });
    return items[k + 1];
  }

  /** Clones the given rows and resolves true when every one finished successfully. */
  async cloneAndWait(items: SetItem[]): Promise<boolean> {
    let release!: () => void;
    const finished = new Promise<void>(resolve => { release = resolve; });
    this.#cloneWaiters.push(release);
    await this.startClone(items, 'clone');
    if (this.running) await finished;
    this.#cloneWaiters = this.#cloneWaiters.filter(waiter => waiter !== release);
    return items.every(item => this.jobs[item.id]?.phase === 'done');
  }

  /** Pushes the current branch of each repo; branches without an upstream are published. Never forces. */
  async pushRepos(targets: { path: string; name: string }[]) {
    if (!targets.length || this.gitBusy || this.running) return;
    if (targets.length > 1 && !await confirm(`Push the current branch of ${targets.length} repositories?`, { title: 'Push', okLabel: 'Push' })) return;
    this.gitBusy = true;
    const failures: string[] = [];
    try {
      for (const target of targets) {
        try { await api.pushBranch(target.path); } catch (reason) { failures.push(`${target.name}: ${reason}`); }
      }
    } finally { this.gitBusy = false; }
    await this.checkExists(targets.map(target => target.path));
    const pushedPaths = new Set(targets.map(target => target.path));
    const urls = [...new Set(this.set.items.filter(item => pushedPaths.has(this.dest(item))).map(item => item.url))];
    if (urls.length) void this.ensureRefs(urls, true);
    const pushed = targets.length - failures.length;
    if (failures.length) {
      this.toast(`Push failed for ${failures.length} of ${targets.length}. ${failures[0]}`, 'error', { label: 'View activity', run: () => { this.activityOpen = true; } });
    } else {
      this.toast(pushed === 1 ? `Pushed ${targets[0].name}` : `Pushed ${pushed} repositories`, 'success');
    }
  }

  /** Deletes a local branch after confirmation. The remote branch is never touched. */
  async deleteLocalBranch(path: string, repo: string, name: string) {
    if (this.gitBusy || this.running) return;
    const ok = await confirm(`Delete the local branch "${name}" in ${repo}?\n\nOnly your local copy is removed. A branch with the same name on the remote, if there is one, is not touched.`,
      { title: 'Delete local branch', kind: 'warning', okLabel: 'Delete', destructive: true });
    if (!ok) return;
    this.gitBusy = true;
    try {
      let force = false;
      for (;;) {
        try {
          const result = await api.deleteBranch(path, name, force);
          this.toast(`Deleted ${name} (was ${result.sha}). The remote branch is untouched.`, 'success');
          break;
        } catch (reason) {
          const text = String(reason);
          if (force || !text.includes('not fully merged')) { this.toast(text, 'error'); return; }
          const again = await confirm(`"${name}" has commits that are not merged into the branch you are on, and no other branch contains them.\n\nDelete it anyway? Git keeps the commits for a while, so a mistake can still be undone from the reflog.`,
            { title: 'Unmerged branch', kind: 'warning', okLabel: 'Delete anyway', destructive: true });
          if (!again) return;
          force = true;
        }
      }
      await this.checkExists([path]);
    } finally { this.gitBusy = false; }
  }

  goAddRepos() {
    const src = this.sources[0];
    const org = src && this.orgsOf(src)[0];
    this.openView(src && org ? { kind: 'org', source: src.id, org } : { kind: 'settings' });
  }

  openVscode(path: string) {
    api.openInVscode(path).catch(e => this.toast(String(e), 'error'));
  }

  /** Refreshes "on disk" markers and the Local column (branch, ahead/behind, changes) for these folders. */
  async checkExists(dests: string[]) {
    if (!dests.length) return;
    this.#invalidateTrees(dests);
    for (const s of await api.localStatus(dests)) {
      this.exists[s.path] = s.exists;
      this.local[s.path] = s;
    }
    await Promise.all(this.openTreePaths.filter(path => dests.includes(path)).map(path => this.loadTree(path, true)));
  }

  /** True when the folder is already on the ref the row asks for. */
  onRef(item: SetItem) {
    const l = this.local[this.dest(item)];
    if (!l?.repo) return false;
    const { type, name } = item.ref;
    if (type === 'branch') return l.branch === name;
    if (type === 'tag') return l.tag === name;
    return !!l.sha && name.slice(0, 7) === l.sha.slice(0, 7);
  }

  async startClone(items: SetItem[] = this.selected, mode: GitAction = 'clone') {
    if (this.running || !items.length) return;
    if (this.bufferGuards.size && !await this.guardBuffers()) return;
    const jobs = items.map(i => ({ id: i.id, url: i.url, dest: this.dest(i), refType: i.ref.type, refName: i.ref.name }));
    const seen = new Set<string>();
    const dup = jobs.find(j => seen.size === seen.add(j.dest.toLowerCase()).size);
    if (dup) {
      this.toast(`Two rows would clone into ${dup.dest}. Give one of them a different folder name.`, 'warn');
      return;
    }
    if (mode === 'clone' && this.ws.onExisting === 'reclone') {
      const n = jobs.filter(j => this.exists[j.dest]).length;
      const ok = !n || await confirm(
        `${n} folder(s) already exist. They will be renamed to <name>.bak-<timestamp> and cloned fresh.`,
        { title: 'Re-clone', kind: 'warning', okLabel: 'Re-clone', destructive: true });
      if (!ok) return;
    }
    this.#runIds = jobs.map(j => j.id);
    this.#runMode = mode;
    for (const j of jobs) this.jobs[j.id] = { id: j.id, phase: 'queued', pct: 0, msg: 'Waiting for a slot' };
    this.running = true;
    try {
      await api.saveSettings({ sources: this.sources, workspace: this.ws });
      await api.startClone(jobs, { parallel: this.ws.parallel, shallow: this.ws.shallow, onExisting: this.ws.onExisting }, mode);
    } catch (e) {
      this.running = false;
      this.toast(String(e), 'error');
    }
  }

  #finished() {
    this.running = false;
    for (const waiter of this.#cloneWaiters.splice(0)) waiter();
    const done = this.#runIds.map(id => this.jobs[id]).filter(Boolean);
    const failed = done.filter(j => j.phase === 'failed').length;
    const verb = { clone: 'Clone', fetch: 'Fetch', pull: 'Pull', switch: 'Switch' }[this.#runMode];
    this.toast(`${verb} finished: ${done.length - failed} ok${failed ? `, ${failed} failed` : ''}`, failed ? 'error' : 'success',
      failed ? { label: 'View activity', run: () => { this.activityOpen = true; } } : undefined);
    this.#invalidateTrees([...this.#treeGeneration.keys()]);
    this.checkExists(this.set.items.map(i => this.dest(i)));
  }
}

export const app = new AppState();
