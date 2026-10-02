import type { CompareRef, RepositoryTree } from './api';

export type RefChoice = { value: string; label: string; note: string };

// Refs that exist in the given checkouts for one reference kind; a count note shows coverage across several checkouts.
export function refChoices(kind: CompareRef['kind'], trees: Record<string, { data?: RepositoryTree }>, paths: string[]): RefChoice[] {
  const names = new Map<string, { name: string; count: number }>();
  const uniquePaths = [...new Set(paths)];
  for (const path of uniquePaths) {
    const tree = trees[path]?.data;
    if (!tree) continue;
    const refs = kind === 'branch' ? tree.branches
      : kind === 'tag' ? tree.tags
      : kind === 'remoteBranch' ? tree.remotes.flatMap(remote => remote.refs).filter(ref => !ref.symbolic)
      : kind === 'commit' ? [...tree.branches, ...tree.tags, ...tree.remotes.flatMap(remote => remote.refs)] : [];
    const seen = new Set<string>();
    for (const ref of refs) {
      const key = kind === 'commit' ? ref.sha : ref.name;
      if (!key || seen.has(key)) continue;
      seen.add(key);
      const current = names.get(key);
      names.set(key, { name: current?.name ?? ref.name, count: (current?.count ?? 0) + 1 });
    }
  }
  return [...names].sort(([left], [right]) => left.localeCompare(right)).map(([value, entry]) => ({
    value,
    label: value,
    note: uniquePaths.length > 1 ? `${entry.count}/${uniquePaths.length}` : kind === 'commit' ? entry.name : '',
  }));
}
