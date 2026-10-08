import { SvelteSet } from 'svelte/reactivity';
import type { LocalStatus, Repo, RepoSet, SetItem, Source, Workspace } from '../api';
import { isCloned } from '../formation';
import { collectEntries, hostOfItem, hostTree, isRemoteItem, matchesRepoFilter, matchesScope, repoFilterCounts, setEntries, type RepoEntry, type RepoFilter } from '../repositories';
import { usableSection, type RepoSection } from '../repo-sections';
import type { View } from '../workspace';

export type RepositoriesHost = {
  readonly ws: Workspace;
  readonly sources: readonly Source[];
  readonly allRepos: readonly Repo[];
  readonly local: Record<string, LocalStatus>;
  readonly view: View;
  readonly set: RepoSet;
  readonly temporary: { readonly sets: readonly RepoSet[] };
  dest: (item: SetItem, setId?: string) => string;
  collisionKey: (path: string) => string;
  readonly activeTabId: string;
  readonly tabs: readonly { id: string }[];
  openView: (view: View, setId?: string) => void;
  activateTab: (id: string) => void;
  closeTab: (id: string) => Promise<void>;
  addRepo: (repo: Repo, notify: boolean, set: RepoSet) => SetItem | undefined;
  startClone: (items: SetItem[], mode: 'clone') => Promise<void>;
  checkExists: (paths: string[]) => Promise<unknown>;
  toast: (message: string, kind?: 'info' | 'success' | 'warn') => void;
  newSet: () => void;
};

const RECHECK_MS = 300;
const STATUS_CHIPS: readonly RepoFilter[] = ['cloned', 'changes', 'behind'];

export type SetAsk = { items: SetItem[]; anchor: Element; clone: boolean };

/** Every repository the app knows, the filters that narrow the table, and the actions on rows that no set owns yet. */
export class Repositories {
  private readonly app!: RepositoriesHost;
  chip = $state<RepoFilter>('all');
  hostFilter = $state('');
  org = $state('');
  query = $state('');
  page = $state(0);
  /** Scroll offset of the table; filters and paging reset it, Back restores it. */
  scroll = 0;
  /** The tab a repository page was opened from; Back returns to it with its filters and scroll. */
  private origin: string | null = null;
  private remote = new Map<string, SetItem>();
  /** The rows the user ticked on the home list; set tabs keep their own `on` flags. */
  private picked = new SvelteSet<string>();
  private visible: readonly SetItem[] = [];
  /** Asks which set a repository joins before it is cloned or added. */
  askSet = $state<SetAsk | null>(null);
  private checkTimer: ReturnType<typeof setTimeout> | undefined;

  constructor(app: RepositoriesHost) { this.app = app; }

  private setNames = $derived(new Map(this.app.ws.sets.flatMap(set => set.items.map(item => [item.id, set.name] as const))));
  inSetView = $derived(this.app.view.kind === 'set' || this.app.view.kind === 'item');
  private facts = $derived({ sources: this.app.sources, stars: this.app.ws.stars });
  everything = $derived(collectEntries({
    ...this.facts, sets: this.app.ws.sets, repos: this.app.allRepos,
    folderKey: (item, set) => this.app.collisionKey(this.app.dest(item, set.id)), remoteItem: repo => this.remoteItem(repo),
  }));
  entries = $derived(this.inSetView ? setEntries(this.app.set, this.facts) : this.everything);
  scoped = $derived(this.entries.filter(entry => matchesScope(entry, { host: this.hostFilter, org: this.org, query: this.query })));
  counts = $derived(repoFilterCounts(this.scoped.map(entry => ({ local: this.localOf(entry), favorite: entry.favorite }))));
  shown = $derived(this.chip === 'all' ? this.scoped : this.scoped.filter(entry => matchesRepoFilter(this.chip, this.localOf(entry), entry.favorite)));
  selected = $derived(this.inSetView ? this.entries.filter(entry => entry.item.on).map(entry => entry.item) : this.shown.filter(entry => this.picked.has(entry.key)).map(entry => entry.item));
  tempEntries = $derived(this.app.temporary.sets.flatMap(set => setEntries(set, this.facts)));
  /** True while some counted folder has no status yet, so the chip counts are a lower bound. */
  partial = $derived(this.scoped.some(entry => !entry.remoteOnly && !this.localOf(entry)));
  tree = $derived(hostTree(this.everything));
  favorites = $derived(this.everything.filter(entry => entry.favorite));

  isOn(item: SetItem): boolean { return this.inSetView ? item.on : this.picked.has(item.id); }

  setOn(item: SetItem, on: boolean) {
    if (this.inSetView) item.on = on;
    else if (on) this.picked.add(item.id);
    else this.picked.delete(item.id);
  }

  clearSelection() {
    if (this.inSetView) for (const entry of this.entries) entry.item.on = false;
    else this.picked.clear();
  }

  localOf(entry: RepoEntry): LocalStatus | undefined { return this.app.local[this.app.dest(entry.item)]; }

  /** The set whose name a folder path is built from; an item no set holds uses the set in view. */
  setNameOf(item: SetItem, setId?: string): string {
    if (setId) return [...this.app.ws.sets, ...this.app.temporary.sets].find(set => set.id === setId)?.name ?? this.app.set.name;
    return this.setNames.get(item.id) ?? this.app.set.name;
  }

  remoteItem(repo: Repo): SetItem {
    const known = this.remote.get(repo.id);
    if (known) return known;
    const item = $state<SetItem>({ id: `remote:${repo.id}`, repoId: repo.id, url: repo.url, org: repo.org, name: repo.name, ref: { type: 'branch', name: repo.defaultBranch || 'main' }, on: false });
    this.remote.set(repo.id, item);
    return item;
  }

  /** The host of the repository cloned at a folder, for messages that say what a local action leaves alone. */
  hostAt(path: string): string {
    const item = this.app.ws.sets.flatMap(set => set.items).find(entry => this.app.dest(entry) === path);
    return item ? hostOfItem(item, this.app.sources) : '';
  }

  /** The entry a repository page shows: the folder on disk when there is one, else the first. */
  resolve(repoId: string): RepoEntry | undefined {
    const known = [...this.everything, ...this.tempEntries].filter(entry => entry.repoId === repoId);
    return known.find(entry => !entry.remoteOnly && this.localOf(entry)?.repo) ?? known[0];
  }

  setIdOf(item: SetItem): string { return [...this.app.ws.sets, ...this.app.temporary.sets].find(set => set.items.some(entry => entry.id === item.id))?.id ?? this.app.set.id; }

  openRepository(repoId: string, section: RepoSection = 'overview') {
    const entry = this.resolve(repoId);
    if (!entry) { this.app.toast('That repository is not in any source or set', 'warn'); return; }
    if (this.app.view.kind !== 'repo') this.origin = this.app.activeTabId;
    const cloned = !!this.localOf(entry)?.repo;
    this.app.openView({ kind: 'repo', repoId, section: usableSection(section, cloned) }, entry.setIds[0]);
  }

  /** Leaves a repository page for the list it came from and closes the page. */
  back() {
    const page = this.app.activeTabId;
    if (this.app.view.kind !== 'repo') return;
    if (this.origin && this.app.tabs.some(tab => tab.id === this.origin)) this.app.activateTab(this.origin);
    else this.app.openView({ kind: 'repos' });
    void this.app.closeTab(page);
  }

  entryOf(itemId: string): RepoEntry | undefined { return this.everything.find(entry => entry.item.id === itemId); }

  filter(patch: Partial<{ chip: RepoFilter; hostFilter: string; org: string; query: string }>) {
    Object.assign(this, patch);
    this.page = 0;
    this.scroll = 0;
  }

  clearFilters() { this.filter({ chip: 'all', hostFilter: '', org: '', query: '' }); }

  /** A repository the listing has but no set does, and that is not on disk either. */
  isUncloned(item: SetItem): boolean { return isRemoteItem(item) && !isCloned(this.app.local[this.app.dest(item)]); }

  /** In a set tab the set is known; at home the user picks or creates one before anything joins it or is cloned. */
  cloneItems(items: readonly SetItem[]) {
    if (!items.some(item => this.isUncloned(item))) return this.cloneInto(items);
    if (this.inSetView) return this.cloneInto(items, this.app.set);
    this.askSet = { items: [...items], anchor: document.activeElement ?? document.body, clone: true };
    return Promise.resolve();
  }

  cloneRemote(item: SetItem) { return this.cloneItems([item]); }

  /** Rows that no set holds join `set` first, then everything is cloned in one run. */
  async cloneInto(items: readonly SetItem[], set?: RepoSet) {
    const ready = items.flatMap(item => (this.isUncloned(item) && set ? this.promote(item, set) : [item]));
    const joined = ready.length - items.filter(item => !isRemoteItem(item)).length;
    if (joined > 0 && set) this.app.toast(`${joined} added to ${set.name}`, 'info');
    const runnable = ready.filter(item => !isRemoteItem(item));
    if (runnable.length) await this.app.startClone(runnable, 'clone');
  }

  /** A folder already on disk that no set holds: pick the set it joins. */
  adopt(items: readonly SetItem[]) { this.askSet = { items: [...items], anchor: document.activeElement ?? document.body, clone: false }; }

  private promote(item: SetItem, set: RepoSet): SetItem[] {
    const repo = this.app.allRepos.find(entry => entry.id === item.repoId);
    const added = repo && this.app.addRepo(repo, false, set);
    return added ? [added] : [];
  }

  addToSet(items: readonly SetItem[], set: RepoSet) {
    const have = new Set(set.items.map(item => item.repoId));
    const fresh = [...new Map(items.filter(item => !have.has(item.repoId)).map(item => [item.repoId, item])).values()];
    const added = fresh.filter(item => { const repo = this.app.allRepos.find(known => known.id === item.repoId); return repo && this.app.addRepo(repo, false, set); });
    const skipped = items.length - added.length;
    this.app.toast(`${added.length} added to ${set.name}${skipped ? `, ${skipped} already there or unavailable` : ''}`, added.length ? 'success' : 'warn');
  }

  newSet(items: readonly SetItem[] = [], clone = false) {
    this.app.newSet();
    if (!items.length) return;
    if (clone) void this.cloneInto(items, this.app.set);
    else this.addToSet(items, this.app.set);
  }

  /** The table tells the store which rows it shows; only those, the ticked rows and, for a status chip, the folders that chip must judge are read. */
  setVisible(items: readonly SetItem[]) {
    this.visible = items;
    this.refreshStatus();
  }

  /** Reads the status of the rows in view and the ticked rows; `force` reads known ones again (window focus). */
  refreshStatus(force = false) {
    clearTimeout(this.checkTimer);
    this.checkTimer = setTimeout(() => {
      const judged = STATUS_CHIPS.includes(this.chip) ? this.everything.filter(entry => !entry.remoteOnly).map(entry => entry.item) : [];
      const paths = [...new Set([...this.visible, ...this.selected, ...judged].map(item => this.app.dest(item)))];
      const due = force ? paths : paths.filter(path => !this.app.local[path]);
      if (due.length) void this.app.checkExists(due);
    }, RECHECK_MS);
  }
}
