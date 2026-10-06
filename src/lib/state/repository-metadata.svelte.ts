import { api, type Commit, type Repo, type SetItem, type Source } from '../api';
import { ForegroundRequests } from './foreground-requests';
import { commitHistoryKey, sourceFingerprint } from './metadata-keys';

export type RefsEntry = {
  branches: string[]; tags: string[]; branchShas?: string[]; tagShas?: string[]; branchLabels?: string[]; tagLabels?: string[]; error?: string; loading?: boolean;
};
export type CommitsEntry = Commit[] | 'loading' | { error: string };
export type RefState = 'ok' | 'missing' | 'unknown' | 'unverified';

const isHex = (s: string) => /^[0-9a-f]{7,40}$/i.test(s);

export class RepositoryMetadata {
  repos = $state<Record<string, Repo[]>>({});
  repoErrors = $state<Record<string, string[]>>({});
  loadingRepos = $state<Record<string, boolean>>({});
  refs = $state<Record<string, RefsEntry>>({});
  commits = $state<Record<string, CommitsEntry>>({});
  allRepos = $derived(Object.values(this.repos).flat());
  repoById = $derived(new Map(this.allRepos.map(r => [r.id, r])));
  private revisions = $state<Record<string, number>>({});
  private refEpochs = $state<Record<string, number>>({});
  private listingEpochs = new Map<string, number>();
  private listingScopes = new Map<string, string>();
  private loadingListingKeys = new Map<string, string>();
  private refScopes = new Map<string, string>();
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
      this.commitOwners.delete(key);
    }
    delete this.repos[sourceId];
    delete this.repoErrors[sourceId];
    this.listingScopes.delete(sourceId);
    this.loadingListingKeys.delete(sourceId);
    this.loadingRepos[sourceId] = false;
  }

  invalidateAll() {
    this.invalidateRefs([...Object.keys(this.refs), ...this.sources().flatMap(source => this.urlsOf(source.id))]);
  }

  invalidateRefs(urls: string[]) {
    for (const url of new Set(urls)) {
      this.refEpochs[url] = (this.refEpochs[url] ?? 0) + 1;
      delete this.refs[url];
      this.refScopes.delete(url);
      for (const [key, owner] of this.commitOwners) {
        if (owner.url === url) {
          delete this.commits[key];
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
      produce: () => api.listRepos($state.snapshot(src) as Source, refresh, this.revisions[src.id] ?? 0),
      publish: list => { if (current()) {
        this.repos[src.id] = list.repos;
        this.repoErrors[src.id] = list.errors;
        this.listingScopes.set(src.id, currentScope);
      } },
      fail: error => { if (current()) {
        delete this.repos[src.id];
        this.repoErrors[src.id] = [String(error)];
      } },
      settled: () => { if (this.loadingListingKeys.get(src.id) === key) {
        this.loadingRepos[src.id] = false;
        this.loadingListingKeys.delete(src.id);
      } },
    });
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
    await Promise.all([...new Set(urls)].map(url => this.loadRefs(url, force, signal)));
  }

  private loadRefs(url: string, force: boolean, signal?: AbortSignal) {
    if (force) this.invalidateRefs([url]);
    const key = this.refsKey(url);
    const cached = this.refs[url];
    if (!force && cached && !cached.loading && !cached.error && this.refScopes.get(url) === key) return Promise.resolve();
    this.refs[url] = { branches: [], tags: [], loading: true };
    const current = () => this.refsKey(url) === key;
    return this.requests.run(key, { signal,
      produce: () => api.getRefsMany([url]),
      publish: rows => { if (current()) {
        const row = rows.find(row => row.url === url);
        if (!row) throw new Error(`No references returned for ${url}`);
        this.refs[url] = { branches: row.branches, tags: row.tags, branchShas: row.branchShas, tagShas: row.tagShas,
          ...(row.branchLabels ? { branchLabels: row.branchLabels } : {}), ...(row.tagLabels ? { tagLabels: row.tagLabels } : {}), error: row.error ?? undefined };
        this.refScopes.set(url, key);
      } },
      fail: error => { if (current()) this.refs[url] = { branches: [], tags: [], error: String(error) }; },
      settled: () => { if (this.refs[url]?.loading && !this.requests.has(this.refsKey(url))) delete this.refs[url]; },
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
    if (Array.isArray(this.commits[key])) return Promise.resolve();
    if (!repo || !src) {
      this.commits[key] = { error: 'Repository metadata is unavailable; refresh its source and retry' };
      return Promise.resolve();
    }
    if (src.kind === 'manual') { this.commits[key] = []; return Promise.resolve(); }
    this.commitOwners.set(key, { source: src.id, url: item.url ?? repo.url ?? '' });
    this.commits[key] = 'loading';
    const current = () => this.commitKey(requested) === key;
    return this.requests.run(key, { signal,
      produce: () => api.getCommits($state.snapshot(src) as Source, repo.org, repo.name, branch),
      publish: commits => { if (current()) this.commits[key] = commits; },
      fail: error => { if (current()) this.commits[key] = { error: String(error) }; },
      settled: () => { if (this.commits[key] === 'loading') delete this.commits[key]; },
    });
  }
}
