import type { RepoSet, SetItem, ShellLayout } from './api';

export type ScopedModule = 'prs' | 'actions';
export type ScopeMode = 'set' | 'all';
export type ScopeMap = Partial<Record<ScopedModule, ScopeMode>>;

export const SCOPED_MODULES: readonly ScopedModule[] = ['prs', 'actions'];
export const isScopedModule = (value: unknown): value is ScopedModule => SCOPED_MODULES.includes(value as ScopedModule);

export const scopeOf = (shell: Pick<ShellLayout, 'scope'>, module: ScopedModule): ScopeMode => shell.scope?.[module] ?? 'set';

export const otherScope = (mode: ScopeMode): ScopeMode => (mode === 'set' ? 'all' : 'set');

export function toggleScope(shell: Pick<ShellLayout, 'scope'>, module: ScopedModule): ScopeMode {
  const next = otherScope(scopeOf(shell, module));
  shell.scope = { ...shell.scope, [module]: next };
  return next;
}

/** Keeps only known modules with known modes, so an old or hand-edited workspace loads. */
export function cleanScope(raw: unknown): ScopeMap {
  const clean: ScopeMap = {};
  if (!raw || typeof raw !== 'object') return clean;
  for (const module of SCOPED_MODULES) {
    const mode = (raw as Record<string, unknown>)[module];
    if (mode === 'set' || mode === 'all') clean[module] = mode;
  }
  return clean;
}

/** The repositories a scoped page reads: the active set, or every saved set without repeating a folder; `folderKey` folds paths that name one folder. */
export function scopeItems(mode: ScopeMode, active: Pick<RepoSet, 'items'>, sets: readonly Pick<RepoSet, 'items'>[], folderKey: (item: SetItem) => string): SetItem[] {
  if (mode === 'set') return active.items;
  const seen = new Set<string>();
  return sets.flatMap(set => set.items).filter(item => {
    const key = folderKey(item);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

/** Folders no status read has answered or failed for; the "all" scope reads them so their branch is known. */
export const unreadPaths = (paths: readonly string[], local: Record<string, unknown>, failures: Record<string, unknown>) => paths.filter(path => !local[path] && !failures[path]);

/** The paths not asked for yet; they join `requested`, so a later chunk of results never re-asks folders still in flight. */
export function takeUnrequested(paths: readonly string[], requested: Set<string>): string[] {
  const fresh = paths.filter(path => !requested.has(path));
  for (const path of fresh) requested.add(path);
  return fresh;
}

export const scopeLabel = (mode: ScopeMode, setName: string) => (mode === 'set' ? `In ${setName}` : 'All repositories');
