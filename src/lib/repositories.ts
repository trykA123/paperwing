import type { LocalStatus, Repo, RepoSet, SetItem, Source } from './api';
import { isCloned } from './formation';

export type RepoEntry = {
  key: string; repoId: string; name: string; org: string; host: string; url: string; defaultBranch: string;
  item: SetItem; setIds: string[]; remoteOnly: boolean; favorite: boolean;
};

export type RepoFilter = 'all' | 'cloned' | 'changes' | 'behind' | 'favorites';
export type RepoFilterCounts = Record<RepoFilter, number>;
export type RepoScope = { host: string; org: string; query: string };

export const REPO_FILTERS: readonly { id: RepoFilter; label: string }[] = [
  { id: 'all', label: 'All' }, { id: 'cloned', label: 'Cloned' }, { id: 'changes', label: 'Has changes' },
  { id: 'behind', label: 'Behind' }, { id: 'favorites', label: 'Favorites' },
];

export const REMOTE_PREFIX = 'remote:';
export const isRemoteItem = (item: SetItem) => item.id.startsWith(REMOTE_PREFIX);

export function hostOf(url: string, source?: Pick<Source, 'host'>): string {
  const named = source?.host.trim().toLowerCase();
  if (named) return named;
  try { return new URL(url).hostname.toLowerCase(); } catch { return url.split('/')[2]?.toLowerCase() ?? ''; }
}

export type CollectFacts = {
  sets: readonly RepoSet[]; repos: readonly Repo[]; sources: readonly Source[]; stars: readonly string[];
  /** Two items with the same key are one folder on disk. */
  folderKey: (item: SetItem, set: RepoSet) => string;
  remoteItem: (repo: Repo) => SetItem;
};

const sourceOf = (repoId: string, sources: readonly Source[]) => sources.find(source => source.id === repoId.split(':')[0]);
export const hostOfItem = (item: Pick<SetItem, 'url' | 'repoId'>, sources: readonly Source[]) => hostOf(item.url, sourceOf(item.repoId, sources));
const byName = (a: Repo, b: Repo) => a.name.localeCompare(b.name) || a.org.localeCompare(b.org);

type EntryFacts = Pick<CollectFacts, 'sources' | 'stars'>;

function entryOf(item: SetItem, setIds: string[], remoteOnly: boolean, facts: EntryFacts, defaultBranch = item.ref.name): RepoEntry {
  return {
    key: item.id, repoId: item.repoId, name: item.name, org: item.org, host: hostOfItem(item, facts.sources), url: item.url,
    defaultBranch, item, setIds, remoteOnly, favorite: facts.stars.includes(item.repoId),
  };
}

/** Every repository the app knows: one entry per folder of the sets, then the listed repositories that no set has. */
export function collectEntries(facts: CollectFacts): RepoEntry[] {
  const folders = new Map<string, RepoEntry>();
  for (const set of facts.sets) {
    for (const item of set.items) {
      const key = facts.folderKey(item, set);
      const known = folders.get(key);
      if (known) { if (!known.setIds.includes(set.id)) known.setIds.push(set.id); continue; }
      folders.set(key, entryOf(item, [set.id], false, facts));
    }
  }
  const owned = new Set([...folders.values()].map(entry => entry.repoId));
  const remote = facts.repos.filter(repo => !owned.has(repo.id)).sort((a, b) => Number(facts.stars.includes(b.id)) - Number(facts.stars.includes(a.id)) || byName(a, b));
  return [...folders.values(), ...remote.map(repo => entryOf(facts.remoteItem(repo), [], true, facts, repo.defaultBranch))];
}

/** The items of one set, as they are today: one entry per row, in the set's order. */
export function setEntries(set: RepoSet, facts: EntryFacts): RepoEntry[] {
  return set.items.map(item => entryOf(item, [set.id], false, facts));
}

export function matchesRepoFilter(filter: RepoFilter, local: LocalStatus | undefined, favorite: boolean): boolean {
  switch (filter) {
    case 'all': return true;
    case 'cloned': return isCloned(local);
    case 'changes': return isCloned(local) && local!.dirty > 0;
    case 'behind': return isCloned(local) && local!.behind > 0;
    case 'favorites': return favorite;
  }
}

export function repoFilterCounts(rows: readonly { local: LocalStatus | undefined; favorite: boolean }[]): RepoFilterCounts {
  const counts: RepoFilterCounts = { all: rows.length, cloned: 0, changes: 0, behind: 0, favorites: 0 };
  for (const row of rows) for (const { id } of REPO_FILTERS) if (id !== 'all' && matchesRepoFilter(id, row.local, row.favorite)) counts[id] += 1;
  return counts;
}

export function matchesScope(entry: Pick<RepoEntry, 'host' | 'org' | 'name'>, scope: RepoScope): boolean {
  if (scope.host && entry.host !== scope.host) return false;
  if (scope.org && entry.org !== scope.org) return false;
  const words = scope.query.toLowerCase().split(/\s+/).filter(Boolean);
  const text = `${entry.org}/${entry.name}`.toLowerCase();
  return words.every(word => text.includes(word));
}

export type HostTree = { host: string; count: number; orgs: { org: string; count: number }[] }[];

/** Hosts with their organizations, github.com first, for the sidebar tree. */
export function hostTree(entries: readonly Pick<RepoEntry, 'host' | 'org'>[]): HostTree {
  const hosts = new Map<string, Map<string, number>>();
  for (const { host, org } of entries) {
    const orgs = hosts.get(host) ?? new Map<string, number>();
    orgs.set(org, (orgs.get(org) ?? 0) + 1);
    hosts.set(host, orgs);
  }
  return [...hosts].map(([host, orgs]) => ({ host, count: [...orgs.values()].reduce((a, b) => a + b, 0), orgs: [...orgs].map(([org, count]) => ({ org, count })).sort((a, b) => a.org.localeCompare(b.org)) }))
    .sort((a, b) => Number(b.host === 'github.com') - Number(a.host === 'github.com') || a.host.localeCompare(b.host));
}
