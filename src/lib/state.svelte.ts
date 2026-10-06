import { Credentials } from './state/credentials.svelte';
import { RepositoryMetadata, type RefState } from './state/repository-metadata.svelte';
import { GitActivity } from './state/git-activity.svelte';
import { RepositoryTrees } from './state/repository-trees.svelte';
import { RootProbes } from './state/root-probes.svelte';
import { railClick } from './rail';
import { doingWord, RunNotices } from './state/run-notices';
import { TemporarySets } from './state/temporary-sets.svelte';
import { destination, folderOf, pathClashes, collisionKey, segments, uniqueFolder } from './workspace-paths';
import { pendingPlatform, unavailableRoot } from './platform';
import { benchmarkEnabled, benchmarkPlan } from './benchmark';
import { listen } from '@tauri-apps/api/event';
import {
    api, type Activity,
    type GitAction, type LocalStatus, type Phase, type Progress, type Ref, type Repo,
    type Capability, type Capabilities, type CompareEndpoint, type PathIdentity, type PlatformInfo, type RootSupport,
    type RailSection, type RepoSet, type SetItem, type Source, type Workspace,
} from './api';
import { CompareState, SetCompareState, type SetCompareRow } from './compare.svelte';
import { confirm } from './confirm';
import { defaultWorkspace, migrateWorkspace, tabId, type ShellTab, type View } from './workspace';
import { NotificationStore, type NoticeAction, type NoticeKind, type NoticeOptions } from './notifications.svelte';
export { DEFAULT_COLS, DEFAULT_TEMPLATE } from './workspace';
export type { View } from './workspace';

export type { RefsEntry, CommitsEntry, RefState } from './state/repository-metadata.svelte';
export const uid = () => Math.random().toString(36).slice(2, 10);
export const RUNNING: Phase[] = ['resolving', 'cloning', 'fetching', 'checkout'];
export const PHASE: Record<Phase, string> = {
  queued: 'Queued', resolving: 'Starting', cloning: 'Cloning', fetching: 'Fetching',
  checkout: 'Checkout', done: 'Done', failed: 'Failed', skipped: 'Skipped',
};
export const refText = (r: Ref) => (r.type === 'commit' ? r.name.slice(0, 8) : r.name);
export const matches = (text: string, query: string) =>
  query.toLowerCase().split(/\s+/).filter(Boolean).every(w => text.toLowerCase().includes(w));

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
  platform = $state<PlatformInfo>(pendingPlatform);
  private rootProbing = new RootProbes(() => this.#invalidateTrees(this.repositoryTrees.paths()));
  get rootProbes() { return this.rootProbing.probes; }
  set rootProbes(value: RootProbes['probes']) { this.rootProbing.probes = value; }
  pathIdentities = $state<Record<string, PathIdentity>>({});
  private identityRevision = 0;
  get nativePlatform() { return this.platform.platform === 'linux' ? 'linux' : 'windows'; }
  get rootSupport() { return this.rootProbes[this.ws.root] ?? unavailableRoot(this.ws.root, 'Choose a valid native destination folder.'); }
  capability(operation: keyof Capabilities, root = this.ws.root): Capability {
    const capability = this.platform.capabilities[operation];
    if (!capability.supported || operation === 'recovery') return capability;
    return (this.rootProbes[root] ?? unavailableRoot(root, 'Choose a valid native destination folder.')).capabilities[operation];
  }
  probeRoot(root = this.ws.root) { return this.rootProbing.probe(root); }
  async chooseRoot(root: string) {
    if (this.running || this.clonePreparing || this.gitBusy || !await this.guardBuffers()) return false;
    const support = await this.probeRoot(root);
    if (!support.valid) { this.toast(support.reason ?? 'Choose a valid native destination folder.', 'warn'); return false; }
    this.#invalidateTrees(this.repositoryTrees.paths());
    this.ws.root = root;
    this.pathIdentities = {};
    return true;
  }
  endpointRoot(endpoint: CompareEndpoint) {
    const item = this.ws.sets.find(set => set.id === endpoint.setId)?.items.find(item => item.id === endpoint.itemId);
    return item ? this.dest(item, endpoint.setId) : '';
  }
  endpointCapability(endpoint: CompareEndpoint, operation: keyof Capabilities) {
    return this.capability(operation, this.endpointRoot(endpoint));
  }
  async probeEndpoints(endpoints: CompareEndpoint[]) {
    await Promise.all([...new Set(endpoints.map(endpoint => this.endpointRoot(endpoint)))].map(root => this.probeRoot(root)));
  }
  async refreshPathIdentities(paths: string[]) {
    const current = ++this.identityRevision;
    const results = await api.pathIdentities(paths);
    if (current === this.identityRevision) {
      this.#invalidateTrees(results.filter(result => this.pathIdentities[result.path]?.identity !== result.identity).map(result => result.path));
      this.pathIdentities = Object.fromEntries(results.map(result => [result.path, result]));
    }
    return results;
  }
  collisionKey(path: string) { return collisionKey(path, this.nativePlatform, this.pathIdentities); }
  copyRequest = $state<{ comparisonId: string; id: string; generation: number; fileId: string; side: 'left' | 'right' } | null>(null);
  recoveryOpen = $state(false);
  readonlyBenchmark = false;
  gitDialog = $state<{ kind: 'commit' | 'branch'; path: string; name: string; itemId?: string; targets?: { path: string; name: string }[] } | null>(null);
  /** True while a push or branch deletion runs; clone jobs use `running` instead. */
  gitBusy = $state(false);
  copyActions = $state<Record<string, { left: boolean; right: boolean; copy: (side: 'left' | 'right') => void; leftReason?: string | null; rightReason?: string | null }>>({});
  async requestCopy(comparisonId: string, fileId: string, side: 'left' | 'right') {
    const snapshot = this.comparisons[comparisonId]?.snapshot;
    if (!snapshot || this.copyRequest) return;
    const endpoint = snapshot[side].endpoint;
    if (!this.platform.capabilities.copy.supported) { this.toast(this.platform.capabilities.copy.reason ?? 'Copy is unavailable.', 'warn'); return; }
    await this.probeEndpoints([endpoint]);
    const capability = this.endpointCapability(endpoint, 'copy');
    if (!capability.supported) { this.toast(capability.reason ?? 'Copy is unavailable for this root.', 'warn'); return; }
    if (this.copyRequest || this.comparisons[comparisonId]?.snapshot !== snapshot) return;
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
    canSave: boolean; canSaveLeft: boolean; canSaveRight: boolean; canNavigate: boolean; canUndo: boolean; undo: () => Promise<void>; saveReasons?: (string | null)[]; undoReason?: string | null }>>({});
  async guardBuffers(ids?: Set<string>) {
    for (const [id, guard] of this.bufferGuards) if ((!ids || ids.has(id)) && !await guard()) return false;
    return true;
  }
  ready = $state(false);
  sources = $state<Source[]>([]);
  ws = $state<Workspace>(defaultWorkspace('unsupported'));
  private repositoryMetadata = new RepositoryMetadata(() => this.sources, () => this.ws.sets.flatMap(set => set.items));
  credentials = new Credentials(sourceId => {
    const paths = this.ws.sets.flatMap(set => set.items.filter(item => item.repoId.startsWith(`${sourceId}:`)).map(item => this.dest(item, set.id)));
    this.#invalidateTrees(paths);
    this.repositoryMetadata.invalidateSource(sourceId);
  });
  get repos() { return this.repositoryMetadata.repos; }
  set repos(value: RepositoryMetadata['repos']) { this.repositoryMetadata.repos = value; }
  get repoErrors() { return this.repositoryMetadata.repoErrors; }
  set repoErrors(value: RepositoryMetadata['repoErrors']) { this.repositoryMetadata.repoErrors = value; }
  get staleRepos() { return this.repositoryMetadata.staleRepos; }
  get loadingRepos() { return this.repositoryMetadata.loadingRepos; }
  set loadingRepos(value: RepositoryMetadata['loadingRepos']) { this.repositoryMetadata.loadingRepos = value; }
  get refs() { return this.repositoryMetadata.refs; }
  set refs(value: RepositoryMetadata['refs']) { this.repositoryMetadata.refs = value; }
  get commits() { return this.repositoryMetadata.commits; }
  set commits(value: RepositoryMetadata['commits']) { this.repositoryMetadata.commits = value; }
  exists = $state<Record<string, boolean>>({});
  local = $state<Record<string, LocalStatus>>({});
  jobs = $state<Record<string, Progress>>({});
  running = $state(false);
  clonePreparing = $state(false);
  get activityOpen() { return this.ws.shell.sidebarVisible && this.ws.shell.section === 'activity'; }
  set activityOpen(open: boolean) {
    if (open) { this.ws.shell.section = 'activity'; this.ws.shell.sidebarVisible = true; }
    else if (this.activityOpen) this.ws.shell.sidebarVisible = false;
  }
  clickRail(section: RailSection) { Object.assign(this.ws.shell, railClick(this.ws.shell, section)); }
  private gitActivity = new GitActivity();
  get activity() { return this.gitActivity.activity; }
  set activity(value: Activity[]) { this.gitActivity.activity = value; }
  private repositoryTrees = new RepositoryTrees(path => JSON.stringify([this.ws.root,
    this.rootProbing.identity(this.ws.root), this.pathIdentities[path]?.identity ?? '']));
  get trees() { return this.repositoryTrees.trees; }
  set trees(value: RepositoryTrees['trees']) { this.repositoryTrees.trees = value; }
  openTreePaths: string[] = [];
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
  notices = new NotificationStore();
  private runNotices = new RunNotices(this.notices);
  temporary = new TemporarySets({
    get ws() { return app.ws; }, get tabs() { return app.tabs; },
    get activeTabId() { return app.activeTabId; }, set activeTabId(id: string) { app.activeTabId = id; },
    openView: (view, setId) => this.openView(view, setId), closeTab: id => this.closeTab(id),
    toast: (message, kind, action) => this.toast(message, kind, action), newId: uid,
  });
  pendingRename = $state(false);
  renameItemId = $state<string | null>(null);
  #runIds = $state<string[]>([]);
  #runMode: GitAction = 'clone';
  #runItems: SetItem[] = [];
  #runSetId = '';
  #runNotice = 0;
  pushing = $state<Record<string, 'Pushing' | 'Waiting'>>({});
  #cloneWaiters: (() => void)[] = [];

  get allRepos() { return this.repositoryMetadata.allRepos; }
  set allRepos(value: RepositoryMetadata['allRepos']) { this.repositoryMetadata.allRepos = value; }
  get repoById() { return this.repositoryMetadata.repoById; }
  set repoById(value: RepositoryMetadata['repoById']) { this.repositoryMetadata.repoById = value; }
  set = $derived<RepoSet>(this.ws.sets.find(s => s.id === this.ws.activeSet) ?? this.temporary.find(this.ws.activeSet) ?? this.ws.sets[0]);
  isTemporary = $derived(!!this.temporary.find(this.set.id));
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
  inspectedId = $state<string | null>(null);
  /** When each set last finished a clean fetch this session (ms since epoch). */
  lastFetch = $state<Record<string, number>>({});
  /** The repository the right panel describes: the focused row, else the last row clicked, else the only selected one. */
  detailItem = $derived(this.focusedItem ?? this.set.items.find(item => item.id === this.inspectedId) ?? (this.selected.length === 1 ? this.selected[0] : undefined));
  clashes = $derived(pathClashes(this.selected.map(item => this.dest(item)), this.nativePlatform, this.pathIdentities));

  /** What is happening to this row right now, for its spinner; null when nothing is. */
  rowBusy(item: SetItem): string | null {
    const push = this.pushing[this.dest(item)];
    if (push) return push === 'Waiting' ? 'Waiting' : 'Pushing';
    const job = this.jobs[item.id];
    if (!this.running || !job || !this.#runIds.includes(item.id)) return null;
    if (job.phase === 'queued') return 'Waiting';
    return RUNNING.includes(job.phase) ? doingWord(this.#runMode) : null;
  }

  async init() {
    const [saved, platform] = await Promise.all([api.loadSettings(), api.platformInfo()]);
    this.platform = platform;
    const ws = migrateWorkspace(saved.workspace, platform.platform);
    this.sources = saved.sources ?? [];
    this.ws = ws;
    await this.probeRoot();
    this.openView({ kind: 'set' });
    await listen<Progress>('clone-progress', e => { this.jobs[e.payload.id] = e.payload; });
    await listen('clone-finished', () => this.#finished());
    await listen<{ sourceId: string; revision: number }>('credential-changed', event => this.credentials.invalidate(event.payload.sourceId, event.payload.revision));
    await listen<Activity>('git-activity', event => this.mergeActivity(event.payload));
    await this.refreshActivity();
    this.ready = true;
    if (!this.sources.length) this.openView({ kind: 'settings' });
    await Promise.all(this.sources.map(s => this.loadRepos(s, false)));
    if (benchmarkEnabled) {
      const plan = await benchmarkPlan();
      this.readonlyBenchmark = plan.scenario === 'linux-read-only';
      const comparisonId = 'fixture-benchmark';
      this.comparisons[comparisonId] = new CompareState();
      this.openView({ kind: 'compare', comparisonId, left: plan.left, right: plan.right });
    }
  }

  toast(msg: string, kind: NoticeKind = 'info', action?: NoticeAction, options: NoticeOptions = {}) {
    return this.notices.notify(msg, kind, { ...options, actions: [...(action ? [action] : []), ...(options.actions ?? [])] });
  }

  mergeActivity(entry: Activity) { return this.gitActivity.mergeActivity(entry); }

  refreshActivity() { return this.gitActivity.refreshActivity(); }

  clearActivity() { return this.gitActivity.clearActivity(); }

  loadTree(path: string, force = false, signal?: AbortSignal) { return this.repositoryTrees.loadTree(path, force, signal); }

  readTree(path: string, signal?: AbortSignal) { return this.repositoryTrees.readTree(path, signal); }

  #invalidateTrees(paths: string[]) { this.repositoryTrees.invalidate(paths); }

  loadRepos(src: Source, refresh: boolean, signal?: AbortSignal) { return this.repositoryMetadata.loadRepos(src, refresh, signal); }

  markMetadataStale() { this.repositoryMetadata.markStale(); }

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
    if (this.ws.sets.some(set => set.id === tab.setId) || this.temporary.find(tab.setId)) this.ws.activeSet = tab.setId;
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
    if (target) this.temporary.releaseIfUnused(target.setId);
    if (!activeRemoved) return;
    const next = this.tabs[Math.min(index, this.tabs.length - 1)];
    if (next) this.activateTab(next.id);
    else this.openView({ kind: 'set' });
  }

  tabTitle(tab: ShellTab) {
    const set = this.ws.sets.find(set => set.id === tab.setId) ?? this.temporary.find(tab.setId);
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

  orgsOf(src: Source) { return this.repositoryMetadata.orgsOf(src); }

  reposOf(sourceId: string, org: string) { return this.repositoryMetadata.reposOf(sourceId, org); }

  /** Folder path below the root, one entry per folder level. */
  segments(item: SetItem, setName = this.set.name): string[] {
    return segments(this.ws, this.sources, item, setName, this.nativePlatform);
  }

  dest(item: SetItem, setId = this.set.id) {
    return destination(this.ws, this.sources, item, this.ws.sets.find(set => set.id === setId)?.name ?? this.set.name, this.nativePlatform);
  }

  folderOf(item: SetItem) { return folderOf(item); }

  hasClash(item: SetItem) {
    return (this.clashes.get(this.collisionKey(this.dest(item))) ?? 0) > 1;
  }

  refState(item: SetItem): RefState { return item.path ? 'ok' : this.repositoryMetadata.refState(item); }

  refStale(item: SetItem) { return this.repositoryMetadata.refs[item.url]?.stale === true; }

  needsRefs(url: string) { return this.repositoryMetadata.needsRefs(url); }

  needsCommits(item: SetItem) { return this.repositoryMetadata.needsCommits(item); }

  ensureRefs(urls: string[], force = false, signal?: AbortSignal) { return this.repositoryMetadata.ensureRefs(urls, force, signal); }

  ensureCommits(item: SetItem, force = false, signal?: AbortSignal) { return this.repositoryMetadata.ensureCommits(item, force, signal); }

  commitKey(item: SetItem) { return this.repositoryMetadata.commitKey(item); }

  commitsFor(item: SetItem) { return this.repositoryMetadata.commitsFor(item); }

  inSet(repoId: string) {
    return this.set.items.some(i => i.repoId === repoId);
  }

  countInSet(repoId: string) {
    return this.set.items.filter(i => i.repoId === repoId).length;
  }

  /** `base`, or `base_2`, `base_3`… — whichever is not yet used in the set. */
  uniqueFolder(base: string) { return uniqueFolder(base, this.set.items, this.nativePlatform); }

  addRepo(repo: Repo, notify = true) {
    if (this.isTemporary) { this.toast('Save this temporary set before adding repositories to it', 'warn'); return; }
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
    const v = this.nativePlatform === 'linux' ? value.replace(/[\/\x00]/g, '')
      : value.replace(/[\\/:*?"<>|\x00-\x1f]/g, '').replace(/^[.\s]+|[.\s]+$/g, '');
    if (v === '.' || v === '..' || v.toLowerCase() === '.git') { this.toast('Choose a safe folder name.', 'warn'); return; }
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
    if (trashFolders && !this.capability('trash').supported) { this.toast(this.capability('trash').reason ?? 'Folder removal is unavailable.', 'warn'); return; }
    if (this.bufferGuards.size && !await this.guardBuffers()) return;
    if (trashFolders && (this.running || this.clonePreparing || this.gitBusy)) { this.toast('Wait for the running Git operation to finish first', 'warn'); return; }
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
    const noticeId = this.runNotices.begin('push', targets.length);
    const failures: string[] = [];
    const failed: { path: string; name: string }[] = [];
    this.pushing = Object.fromEntries(targets.map(target => [target.path, 'Waiting' as const]));
    try {
      for (const target of targets) {
        this.pushing[target.path] = 'Pushing';
        try { await api.pushBranch(target.path); } catch (reason) { failures.push(`${target.name}: ${reason}`); failed.push(target); }
        delete this.pushing[target.path];
      }
    } finally { this.gitBusy = false; this.pushing = {}; }
    await this.checkExists(targets.map(target => target.path));
    const pushedPaths = new Set(targets.map(target => target.path));
    const urls = [...new Set(this.set.items.filter(item => pushedPaths.has(this.dest(item))).map(item => item.url))];
    if (urls.length) void this.ensureRefs(urls, true);
    this.runNotices.finish(noticeId, {
      verb: 'push', total: targets.length, failed, firstError: failures[0],
      retry: again => void this.pushRepos([...again]), viewActivity: () => { this.activityOpen = true; },
    });
  }

  /** Deletes a local branch after confirmation. The remote branch is never touched. */
  async deleteLocalBranch(path: string, repo: string, name: string, label = name) {
    if (this.gitBusy || this.running) return;
    const ok = await confirm(`Delete the local branch "${label}" in ${repo}?\n\nOnly your local copy is removed. A branch with the same name on the remote, if there is one, is not touched.`,
      { title: 'Delete local branch', kind: 'warning', okLabel: 'Delete', destructive: true });
    if (!ok) return;
    this.gitBusy = true;
    try {
      let force = false;
      for (;;) {
        try {
          const result = await api.deleteBranch(path, name, force);
          this.toast(`Deleted ${label} (was ${result.sha}). The remote branch is untouched.`, 'success');
          break;
        } catch (reason) {
          const text = String(reason);
          if (force || !text.includes('not fully merged')) { this.toast(text, 'error'); return; }
          const again = await confirm(`"${label}" has commits that are not merged into the branch you are on, and no other branch contains them.\n\nDelete it anyway? Git keeps the commits for a while, so a mistake can still be undone from the reflog.`,
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
    const paths = new Set(dests);
    const urls = this.ws.sets.flatMap(set => set.items.filter(item => paths.has(this.dest(item, set.id))).map(item => item.url));
    this.repositoryMetadata.invalidateRefs(urls);
    for (const s of await api.localStatus(dests)) {
      this.exists[s.path] = s.exists;
      this.local[s.path] = s;
    }
    await Promise.all(this.openTreePaths.filter(path => dests.includes(path)).map(path => this.loadTree(path, true)));
  }

  refLabel(item: SetItem) {
    if (item.path) { const l = this.local[item.path]; return l?.branchLabel ?? l?.branch ?? l?.tagLabel ?? l?.tag ?? l?.sha?.slice(0, 8) ?? ''; }
    if (item.ref.type === 'commit') return refText(item.ref);
    const refs = this.refs[item.url];
    const names = item.ref.type === 'branch' ? refs?.branches : refs?.tags;
    const labels = item.ref.type === 'branch' ? refs?.branchLabels : refs?.tagLabels;
    const index = names?.indexOf(item.ref.name) ?? -1;
    if (labels?.[index]) return labels[index];
    const path = this.dest(item), local = this.local[path];
    if (item.ref.type === 'branch' && local?.branch === item.ref.name && local.branchLabel) return local.branchLabel;
    if (item.ref.type === 'tag' && local?.tag === item.ref.name && local.tagLabel) return local.tagLabel;
    const tree = this.trees[path]?.data;
    const reference = (item.ref.type === 'branch' ? tree?.branches : tree?.tags)?.find(ref => ref.name === item.ref.name);
    return reference?.label ?? item.ref.name;
  }

  /** True when the folder is already on the ref the row asks for. */
  onRef(item: SetItem) {
    const l = this.local[this.dest(item)];
    if (!l?.repo) return false;
    if (item.path) return true;
    const { type, name } = item.ref;
    if (type === 'branch') return l.branch === name;
    if (type === 'tag') return l.tag === name;
    return !!l.sha && name.slice(0, 7) === l.sha.slice(0, 7);
  }

  async startClone(items: SetItem[] = this.selected, mode: GitAction = 'clone', setId = this.set.id) {
    if (this.running || this.clonePreparing || !items.length) return;
    this.clonePreparing = true;
    try {
      const support = await this.probeRoot();
      if (!support.valid) { this.toast(support.reason ?? 'Choose a valid native destination folder.', 'warn'); return; }
      if (this.bufferGuards.size && !await this.guardBuffers()) return;
      const jobs = items.map(i => ({ id: i.id, url: i.url, dest: this.dest(i, setId), refType: i.ref.type, refName: i.ref.name }));
      let observations: PathIdentity[];
      try { observations = await this.refreshPathIdentities(jobs.map(job => job.dest)); }
      catch (reason) { this.toast(String(reason), 'error'); return; }
      const identities = Object.fromEntries(observations.map(observation => [observation.path, observation]));
      const invalid = jobs.find(job => identities[job.dest]?.reason);
      if (invalid) { this.toast(identities[invalid.dest].reason!, 'warn'); return; }
      const seen = new Set<string>();
      const dup = jobs.find(j => seen.size === seen.add(collisionKey(j.dest, this.nativePlatform, identities)).size);
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
      this.#runItems = [...items];
      this.#runSetId = setId;
      this.#runMode = mode;
      for (const j of jobs) this.jobs[j.id] = { id: j.id, phase: 'queued', pct: 0, msg: 'Waiting for a slot' };
      this.running = true;
      this.#runNotice = this.runNotices.begin(mode, jobs.length);
      try {
        await api.saveSettings({ sources: this.sources, workspace: this.ws });
        await api.startClone(jobs, { parallel: this.ws.parallel, shallow: this.ws.shallow, onExisting: this.ws.onExisting }, mode);
      } catch (e) {
        this.running = false;
        this.notices.dismiss(this.#runNotice);
        this.toast(String(e), 'error');
      }
    } finally { this.clonePreparing = false; }
  }

  #finished() {
    this.running = false;
    for (const waiter of this.#cloneWaiters.splice(0)) waiter();
    const mode = this.#runMode;
    const runSet = this.#runSetId;
    const failed = this.#runItems.filter(item => this.jobs[item.id]?.phase === 'failed');
    this.runNotices.finish(this.#runNotice, {
      verb: mode, total: this.#runItems.length, failed, firstError: this.jobs[failed[0]?.id]?.msg,
      retry: again => void this.startClone([...again], mode, runSet), viewActivity: () => { this.activityOpen = true; },
    });
    this.#invalidateTrees(this.repositoryTrees.paths());
    if (mode === 'fetch' && !failed.length) this.lastFetch[runSet] = Date.now();
    const owner = this.ws.sets.find(set => set.id === runSet) ?? this.temporary.find(runSet) ?? this.set;
    this.checkExists(owner.items.map(i => this.dest(i, owner.id)));
  }
}

export const app = new AppState();
