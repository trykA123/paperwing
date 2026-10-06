import type { FoundRepo, RepoSet, SetItem } from './api';

const parentName = (path: string) => path.split(/[\\/]+/).filter(Boolean).at(-2) ?? '';

export function toSetItem(repo: FoundRepo): SetItem {
  return {
    id: `found:${repo.path}`, repoId: `local:${repo.path}`, url: '', org: parentName(repo.path), name: repo.name,
    ref: repo.branch ? { type: 'branch', name: repo.branch } : { type: 'commit', name: '' }, on: false, path: repo.path,
  };
}

/** The saved copy is independent of the temporary set: new set id, a new id per item, nothing selected. `itemIds` maps old item ids to new ones. */
export function promoteTemporarySet(temporary: { name: string; items: readonly SetItem[] }, id: string, newId: () => string): { set: RepoSet; itemIds: Map<string, string> } {
  const itemIds = new Map<string, string>();
  const items = temporary.items.map(item => {
    const next = newId();
    itemIds.set(item.id, next);
    return { ...item, id: next, ref: { ...item.ref }, on: false };
  });
  return { set: { id, name: temporary.name, items }, itemIds };
}

/** Settings never remember a temporary set as the active one; it would not exist after a restart. */
export function withoutTemporaryActive<T extends { workspace: { activeSet: string; sets: readonly { id: string }[] } }>(settings: T): T {
  if (!isTemporaryId(settings.workspace.activeSet)) return settings;
  return { ...settings, workspace: { ...settings.workspace, activeSet: settings.workspace.sets[0]?.id ?? '' } };
}

export const isTemporaryId = (id: string) => id.startsWith('temp-');
