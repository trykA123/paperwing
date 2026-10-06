import { api, type Commit, type Repo, type RefsResult, type SetItem, type Source } from '../api';
import { ForegroundRequests } from './foreground-requests';
import { commitHistoryKey, sourceFingerprint } from './metadata-keys';

export type RefsEntry = {
  branches: string[]; tags: string[]; branchShas?: string[]; tagShas?: string[]; branchLabels?: string[]; tagLabels?: string[]; error?: string; loading?: boolean; stale?: boolean; failures?: number; retryAt?: number;
};
export type CommitsEntry = Commit[] | 'loading' | { error: string };
export type RefState = 'ok' | 'missing' | 'unknown' | 'unverified';

const RETRY_BASE_MS = 2_000;
const RETRY_MAX_MS = 60_000;
const retryDue = (entry: RefsEntry) => Date.now() >= (entry.retryAt ?? 0);
const isHex = (s: string) => /^[0-9a-f]{7,40}$/i.test(s);

export class RepositoryMetadata {
  repos = $state<Record<string, Repo[]>>({});
  repoErrors = $state<Record<string, string[]>>({});
  loadingRepos = $state<Record<string, boolean>>({});
  staleRepos = $state<Record<string, boolean>>({});
  repoWarnings = $state<Record<string, string[]>>({});
  refs = $state<Record<string, RefsEntry>>({});
  commits = $state<Record<string, CommitsEntry>>({});
  private staleCommits = $state<Record<string, boolean>>({});
  allRepos = $derived(Object.values(this.repos).flat());
  repoById = $derived(new Map(this.allRepos.map(r => [r.id, r])));
  private revisions = $state<Record<string, number>>({});
  private refEpochs = $state<Record<string, number>>({});
  private listingEpochs = new Map<string, number>();
  private listingScopes = new Map<string, string>();
  private loadingListingKeys = new Map<string, string>();
  private refScopes = new Map<string, string>();
  private refFailures = new Map<string, { failures: number; retryAt: number }>();
  private refOwners = new Map<string, Set<string>>();
  private ownersByUrl = $derived.by(() => {
    const owners = new Map<string, Set<string>>();
    const add = (url: string, source: string) => {
      if (!url || !source) return;
      const current = owners.get(url) ?? new Set<string>();
      current.add(source);
      owners.set(url, current);
    };
    for (const repo of this.allRepos) add(repo.url, repo.source);
    for (const item of this.items()) add(item.url, item.repoId.split(':', 1)[0]);
    return owners;
  });
  private commitOwners = new Map<string, { source: string; url: string }>();
  private requests = new ForegroundRequests();

  constructor(private sources: () => Source[], private items: () => SetItem[] = () => []) {}

  private sourceScope(source: Source) {
    return JSON.stringify(['source-v2', source.id, sourceFingerprint(source), this.revisions[source.id] ?? 0]);
  }

  private urlsOf(sourceId: string) {
    const urls = new Set([...this.ownersByUrl].filter(([, owners]) => owners.has(sourceId)).map(([url]) => url));
    for (const [url, owners] of this.refOwners) if (owners.has(sourceId)) urls.add(url);
    return [...urls].filter(Boolean);
  }

  invalidateSource(sourceId: string) {
    this.invalidateRefs(this.urlsOf(sourceId));
    this.revisions[sourceId] = (this.revisions[sourceId] ?? 0) + 1;
    for (const [key, owner] of this.commitOwners) if (owner.source === sourceId) {
      delete this.commits[key];
      delete this.staleCommits[key];
      this.commitOwners.delete(key);
    }
    delete this.repos[sourceId];
    delete this.repoErrors[sourceId];
    delete this.staleRepos[sourceId];
    delete this.repoWarnings[sourceId];
    this.listingScopes.delete(sourceId);
    this.loadingListingKeys.delete(sourceId);
    this.loadingRepos[sourceId] = false;
  }

  markStale(urls?: readonly string[]) {
    const only = urls && new Set(urls);
    for (const [url, entry] of Object.entries(this.refs)) if (!entry.loading && (!only || only.has(url))) entry.stale = true;
    for (const [key, entry] of Object.entries(this.commits)) {
      if (entry === 'loading') continue;
      const owner = this.commitOwners.get(key);
      if (!only || (owner && only.has(owner.url))) this.staleCommits[key] = true;
    }
  }

  needsRefs(url: string) {
    const entry = this.refs[url];
    if (!entry) return true;
    return entry.error ? retryDue(entry) : entry.stale === true;
  }

  needsCommits(item: SetItem) {
    const key = this.commitKey(item);
    return this.commits[key] === undefined || this.staleCommits[key] === true;
  }

  invalidateRefs(urls: string[]) {
    for (const url of new Set(urls)) {
      this.refEpochs[url] = (this.refEpochs[url] ?? 0) + 1;
      delete this.refs[url];
      this.refScopes.delete(url);
      this.refFailures.delete(url);
      for (const [key, owner] of this.commitOwners) {
        if (owner.url === url) {
          delete this.commits[key];
          delete this.staleCommits[key];
          this.commitOwners.delete(key);
        }
      }
    }
  }

  loadRepos(src: Source, refresh: boolean, signal?: AbortSignal) {
    if (signal?.aborted) return Promise.resolve();
    const scope = this.sourceScope(src);
    const previous = this.listingScopes.get(src.id);
    if (previous && previous !== scope) this.invalidateSource(src.id);
    if (refresh) {
      this.invalidateRefs(this.urlsOf(src.id));
      this.listingEpochs.set(src.id, (this.listingEpochs.get(src.id) ?? 0) + 1);
    }
    const currentScope = this.sourceScope(src);
    const key = JSON.stringify(['listing-v2', currentScope, this.listingEpochs.get(src.id) ?? 0]);
    if (!refresh && previous === currentScope && this.repos[src.id] && !this.repoErrors[src.id]?.length) return Promise.resolve();
    const current = () => {
      const configured = this.sources().find(source => source.id === src.id);
      return !!configured && this.sourceScope(configured) === currentScope
        && key === JSON.stringify(['listing-v2', currentScope, this.listingEpochs.get(src.id) ?? 0]);
    };
    this.loadingRepos[src.id] = true;
    this.loadingListingKeys.set(src.id, key);
    return this.requests.run(key, { signal,
      produce: () => {
        const network = api.listRepos($state.snapshot(src) as Source, refresh);
        if (!refresh && !this.repos[src.id]) void this.showCachedRepos(src, current);
        return network;
      },
      publish: list => { if (current()) {
        const offline = list.errors.length > 0 && !list.repos.length && !!this.repos[src.id]?.length;
        if (!offline) this.repos[src.id] = list.repos;
        this.repoErrors[src.id] = list.errors;
        this.staleRepos[src.id] = offline;
        this.repoWarnings[src.id] = list.warnings ?? [];
        this.listingScopes.set(src.id, currentScope);
      } },
      fail: error => { if (current()) {
        this.repoErrors[src.id] = [String(error)];
        this.staleRepos[src.id] = !!this.repos[src.id];
      } },
      settled: () => { if (this.loadingListingKeys.get(src.id) === key) {
        this.loadingRepos[src.id] = false;
        this.loadingListingKeys.delete(src.id);
      } },
    });
  }

  private async showCachedRepos(src: Source, current: () => boolean) {
    const cached = await api.listCachedRepos($state.snapshot(src) as Source).catch(() => null);
    if (!cached || !current() || this.repos[src.id]) return;
    this.repos[src.id] = cached.repos;
    this.repoErrors[src.id] = [];
    this.staleRepos[src.id] = true;
  }

  orgsOf(src: Source) {
    return src.kind === 'manual' ? [...new Set((this.repos[src.id] ?? []).map(r => r.org))] : src.orgs;
  }

  reposOf(sourceId: string, org: string) {
    const o = org.toLowerCase();
    return (this.repos[sourceId] ?? []).filter(r => r.org.toLowerCase() === o);
  }

  private refsKey(url: string) {
    const owners = new Set(this.ownersByUrl.get(url));
    for (const owner of this.refOwners.get(url) ?? []) owners.add(owner);
    this.refOwners.set(url, owners);
    const scopes = this.sources().filter(source => owners.has(source.id)).map(source => this.sourceScope(source)).sort();
    return JSON.stringify(['refs-v2', url, scopes, this.refEpochs[url] ?? 0]);
  }

  refState(item: SetItem): RefState {
    if (item.ref.type === 'commit') {
      const c = this.commitsFor(item);
      if (Array.isArray(c) && c.some(x => x.sha.startsWith(item.ref.name))) return 'ok';
      return isHex(item.ref.name) ? 'unverified' : 'missing';
    }
    const r = this.refs[item.url];
    if (!r || r.loading || r.error || (this.refScopes.has(item.url) && this.refScopes.get(item.url) !== this.refsKey(item.url))) return 'unknown';
    return (item.ref.type === 'branch' ? r.branches : r.tags).includes(item.ref.name) ? 'ok' : 'missing';
  }

  async ensureRefs(urls: string[], force = false, signal?: AbortSignal) {
    if (signal?.aborted) return;
    const unique = [...new Set(urls)];
    const priority = unique.length > 1 ? 'background' : 'foreground';
    await Promise.all(unique.map(url => this.loadRefs(url, { force, priority, signal })));
  }

  private entryFromRow(row: RefsResult, failures: number): RefsEntry {
    const entry: RefsEntry = { branches: row.branches, tags: row.tags, branchShas: row.branchShas, tagShas: row.tagShas,
      ...(row.branchLabels ? { branchLabels: row.branchLabels } : {}), ...(row.tagLabels ? { tagLabels: row.tagLabels } : {}) };
    return row.error ? { ...entry, ...this.failedEntry(row.error, failures + 1), branches: row.branches, tags: row.tags } : entry;
  }

  private failedEntry(error: string, failures: number): RefsEntry {
    const delay = Math.min(RETRY_BASE_MS * 2 ** (failures - 1), RETRY_MAX_MS);
    return { branches: [], tags: [], error, failures, retryAt: Date.now() + delay };
  }

  private loadRefs(url: string, { force, priority, signal }: { force: boolean; priority: 'foreground' | 'background'; signal?: AbortSignal | undefined }) {
    if (force) this.invalidateRefs([url]);
    const key = this.refsKey(url);
    const cached = this.refs[url];
    if (!force && cached && !cached.loading && !cached.error && !cached.stale && this.refScopes.get(url) === key) return Promise.resolve();
    if (!force && cached?.error && !retryDue(cached)) return Promise.resolve();
    const keepData = !!cached?.stale && !cached.loading && !cached.error;
    const failures = cached?.failures ?? this.refFailures.get(url)?.failures ?? 0;
    if (!keepData) this.refs[url] = { branches: [], tags: [], loading: true, ...(failures ? { failures, retryAt: cached?.retryAt ?? this.refFailures.get(url)?.retryAt ?? 0 } : {}) };
    const current = () => this.refsKey(url) === key;
    return this.requests.run(key, { signal, priority,
      produce: () => api.getRefsMany([url]),
      publish: rows => { if (current()) {
        const row = rows.find(row => row.url === url);
        if (!row) throw new Error(`No references returned for ${url}`);
        this.refs[url] = this.entryFromRow(row, failures);
        this.refScopes.set(url, key);
        if (!row.error) this.refFailures.delete(url);
      } },
      fail: error => { if (current() && !keepData) this.refs[url] = this.failedEntry(String(error), failures + 1); },
      settled: () => {
        const entry = this.refs[url];
        if (!entry?.loading || this.requests.has(this.refsKey(url))) return;
        if (entry.failures) this.refFailures.set(url, { failures: entry.failures, retryAt: entry.retryAt ?? 0 });
        delete this.refs[url];
      },
    });
  }

  commitKey(item: SetItem) {
    const repo = this.repoById.get(item.repoId);
    const source = this.sources().find(source => source.id === repo?.source || item.repoId.startsWith(`${source.id}:`));
    const branch = item.ref.type === 'branch' ? item.ref.name : repo?.defaultBranch ?? '';
    return commitHistoryKey({ source: source ? this.sourceScope(source) : '', repository: item.repoId,
      branch, refEpoch: this.refEpochs[item.url ?? repo?.url ?? ''] ?? 0 });
  }

  commitsFor(item: SetItem) { return this.commits[this.commitKey(item)]; }

  ensureCommits(item: SetItem, force = false, signal?: AbortSignal) {
    if (signal?.aborted) return Promise.resolve();
    const repo = this.repoById.get(item.repoId);
    const src = this.sources().find(s => s.id === repo?.source || item.repoId.startsWith(`${s.id}:`));
    if (force) this.invalidateRefs([item.url ?? repo?.url ?? '']);
    const branch = item.ref.type === 'branch' ? item.ref.name : repo?.defaultBranch ?? '';
    const requested = { ...($state.snapshot(item) as SetItem), ref: { type: 'branch' as const, name: branch } };
    const key = this.commitKey(requested);
    if (Array.isArray(this.commits[key]) && !this.staleCommits[key]) return Promise.resolve();
    if (!repo || !src || src.kind === 'manual') {
      delete this.staleCommits[key];
      this.commits[key] = repo && src ? [] : { error: 'Repository metadata is unavailable; refresh its source and retry' };
      return Promise.resolve();
    }
    this.commitOwners.set(key, { source: src.id, url: item.url ?? repo.url ?? '' });
    const keepData = Array.isArray(this.commits[key]);
    if (!keepData) this.commits[key] = 'loading';
    const current = () => this.commitKey(requested) === key;
    return this.requests.run(key, { signal,
      produce: () => api.getCommits($state.snapshot(src) as Source, repo.org, repo.name, branch),
      publish: commits => { if (current()) {
        this.commits[key] = commits;
        delete this.staleCommits[key];
      } },
      fail: error => { if (current() && !keepData) {
        this.commits[key] = { error: String(error) };
        delete this.staleCommits[key];
      } },
      settled: () => { if (this.commits[key] === 'loading') delete this.commits[key]; },
    });
  }
}
