import type { FoundRepo, RepoSet, SetItem } from './api';

const parentName = (path: string) => path.split(/[\\/]+/).filter(Boolean).at(-2) ?? '';

export function toSetItem(repo: FoundRepo): SetItem {
  return {
    id: `found:${repo.path}`, repoId: `local:${repo.path}`, url: '', org: parentName(repo.path), name: repo.name,
    ref: repo.branch ? { type: 'branch', name: repo.branch } : { type: 'commit', name: '' }, on: false, path: repo.path,
  };
}

/** The saved copy is independent of the temporary set: new id, plain items, nothing selected. */
export function promoteTemporarySet(temporary: { name: string; items: readonly SetItem[] }, id: string): RepoSet {
  return { id, name: temporary.name, items: temporary.items.map(item => ({ ...item, ref: { ...item.ref }, on: false })) };
}

export const isTemporaryId = (id: string) => id.startsWith('temp-');
