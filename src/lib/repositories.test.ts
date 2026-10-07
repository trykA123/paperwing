const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import type { LocalStatus, Repo, RepoSet, SetItem, Source } from './api';
import { collectEntries, hostTree, matchesRepoFilter, matchesScope, repoFilterCounts, setEntries, type CollectFacts } from './repositories';

const sources: Source[] = [
  { id: 's1', name: 'Acme', kind: 'ghe', host: 'git.acme.example', orgs: [], urls: [] },
  { id: 's2', name: 'GitHub', kind: 'github', host: 'github.com', orgs: [], urls: [] },
];
const repo = (id: string, org: string, name: string): Repo => ({ id, source: id.split(':')[0], org, name, description: '', url: `https://x/${org}/${name}.git`, defaultBranch: 'main', pushedAt: '', archived: false });
const item = (id: string, r: Repo, folder?: string): SetItem => ({ id, repoId: r.id, url: r.url, org: r.org, name: r.name, ref: { type: 'branch', name: 'main' }, on: false, ...(folder ? { folder } : {}) });
const local = (patch: Partial<LocalStatus>): LocalStatus => ({ path: 'p', exists: true, repo: true, branch: 'main', tag: null, sha: 'a', upstream: 'origin/main', ahead: 0, behind: 0, dirty: 0, error: null, ...patch });

const api = repo('s1:platform/api', 'platform', 'api');
const web = repo('s1:platform/web', 'platform', 'web');
const cli = repo('s2:oss/cli', 'oss', 'cli');
const apiItem = item('i1', api);

function facts(sets: RepoSet[], stars: string[] = []): CollectFacts {
  return {
    sets, repos: [api, web, cli], sources, stars,
    folderKey: entry => (entry.folder ?? entry.name).toLowerCase(),
    remoteItem: r => item(`remote:${r.id}`, r),
  };
}

describe('collecting repositories', () => {
  test('a repository in two sets is one folder and one entry that lists both sets', () => {
    const sets = [{ id: 'a', name: 'A', items: [apiItem] }, { id: 'b', name: 'B', items: [item('i2', api)] }];
    const entries = collectEntries(facts(sets));
    expect(entries.filter(entry => entry.repoId === api.id)).toHaveLength(1);
    expect(entries[0].setIds).toEqual(['a', 'b']);
  });

  test('a copy in its own folder is its own entry', () => {
    const sets = [{ id: 'a', name: 'A', items: [apiItem, item('i2', api, 'api_2')] }];
    expect(collectEntries(facts(sets)).filter(entry => !entry.remoteOnly)).toHaveLength(2);
  });

  test('listed repositories that no set holds follow as remote only, favorites first', () => {
    const entries = collectEntries(facts([{ id: 'a', name: 'A', items: [apiItem] }], [cli.id]));
    expect(entries.map(entry => [entry.name, entry.remoteOnly])).toEqual([['api', false], ['cli', true], ['web', true]]);
    expect(entries[1]).toMatchObject({ favorite: true, host: 'github.com', defaultBranch: 'main' });
  });

  test('a set view lists its rows as they are, one entry per row', () => {
    const set = { id: 'a', name: 'A', items: [apiItem, item('i2', api, 'api_2')] };
    expect(setEntries(set, { sources, stars: [] }).map(entry => entry.item.id)).toEqual(['i1', 'i2']);
  });
});

describe('filters and counts', () => {
  const rows = [
    { local: local({ dirty: 3 }), favorite: true }, { local: local({ behind: 2 }), favorite: false },
    { local: local({}), favorite: false }, { local: undefined, favorite: true }, { local: local({ exists: false, repo: false }), favorite: false },
  ];

  test('chips count cloned, changed, behind and favorite rows', () => {
    expect(repoFilterCounts(rows)).toEqual({ all: 5, cloned: 3, changes: 1, behind: 1, favorites: 2 });
  });

  test('a row that is not on disk is never cloned, changed or behind', () => {
    for (const filter of ['cloned', 'changes', 'behind'] as const) expect(matchesRepoFilter(filter, local({ exists: false, repo: false, dirty: 4, behind: 4 }), false)).toBe(false);
  });

  test('scope narrows by host, organization and every word of the query', () => {
    const entry = { host: 'git.acme.example', org: 'platform', name: 'api-gateway' };
    expect(matchesScope(entry, { host: 'git.acme.example', org: 'platform', query: 'gate api' })).toBe(true);
    expect(matchesScope(entry, { host: 'github.com', org: '', query: '' })).toBe(false);
    expect(matchesScope(entry, { host: '', org: 'mobile', query: '' })).toBe(false);
    expect(matchesScope(entry, { host: '', org: '', query: 'platform/api' })).toBe(true);
  });
});

describe('host tree', () => {
  test('lists github.com first, then hosts by name, with sorted organizations and counts', () => {
    const tree = hostTree([
      { host: 'git.acme.example', org: 'payments' }, { host: 'git.acme.example', org: 'platform' }, { host: 'git.acme.example', org: 'payments' },
      { host: 'github.com', org: 'oss' },
    ]);
    expect(tree.map(host => [host.host, host.count])).toEqual([['github.com', 1], ['git.acme.example', 3]]);
    expect(tree[1].orgs).toEqual([{ org: 'payments', count: 2 }, { org: 'platform', count: 1 }]);
  });
});
