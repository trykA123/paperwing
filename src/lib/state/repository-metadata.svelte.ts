import { api, type Commit, type Repo, type SetItem, type Source } from '../api';

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
  constructor(private sources: () => Source[]) {}
  private revisions = new Map<string, number>();

  invalidateSource(sourceId: string) {
    this.revisions.set(sourceId, (this.revisions.get(sourceId) ?? 0) + 1);
    for (const id of Object.keys(this.commits)) {
      if (id.startsWith(`${sourceId}:`)) delete this.commits[id];
    }
    delete this.repos[sourceId];
    delete this.repoErrors[sourceId];
    this.loadingRepos[sourceId] = false;
  }

  async loadRepos(src: Source, refresh: boolean) {
    const revision = this.revisions.get(src.id) ?? 0;
    this.loadingRepos[src.id] = true;
    try {
      const list = await api.listRepos($state.snapshot(src) as Source, refresh);
      if ((this.revisions.get(src.id) ?? 0) !== revision) return;
      this.repos[src.id] = list.repos;
      this.repoErrors[src.id] = list.errors;
    } catch (e) {
      if ((this.revisions.get(src.id) ?? 0) !== revision) return;
      this.repoErrors[src.id] = [String(e)];
    } finally {
      if ((this.revisions.get(src.id) ?? 0) === revision) this.loadingRepos[src.id] = false;
    }
  }

  orgsOf(src: Source) {
    return src.kind === 'manual' ? [...new Set((this.repos[src.id] ?? []).map(r => r.org))] : src.orgs;
  }

  reposOf(sourceId: string, org: string) {
    const o = org.toLowerCase();
    return (this.repos[sourceId] ?? []).filter(r => r.org.toLowerCase() === o);
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
      this.refs[r.url] = { branches: r.branches, tags: r.tags, branchShas: r.branchShas, tagShas: r.tagShas, ...(r.branchLabels ? { branchLabels: r.branchLabels } : {}), ...(r.tagLabels ? { tagLabels: r.tagLabels } : {}), error: r.error ?? undefined };
    }
  }

  async ensureCommits(item: SetItem) {
    if (this.commits[item.repoId]) return;
    const repo = this.repoById.get(item.repoId);
    const src = this.sources().find(s => s.id === repo?.source);
    if (!repo || !src || src.kind === 'manual') { this.commits[item.repoId] = []; return; }
    this.commits[item.repoId] = 'loading';
    const revision = this.revisions.get(src.id) ?? 0;
    try {
      const branch = item.ref.type === 'branch' ? item.ref.name : repo.defaultBranch;
      const commits = await api.getCommits($state.snapshot(src) as Source, repo.org, repo.name, branch);
      if ((this.revisions.get(src.id) ?? 0) === revision) this.commits[item.repoId] = commits;
    } catch (e) {
      if ((this.revisions.get(src.id) ?? 0) === revision) this.commits[item.repoId] = { error: String(e) };
    }
  }
}
