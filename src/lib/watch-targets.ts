import type { LocalStatus, RepoSet, SetItem } from './api';

type TabLike = { setId: string; view: { kind: string } };

export type WatchTargetInput = {
  sets: readonly RepoSet[];
  tabs: readonly TabLike[];
  local: Readonly<Record<string, LocalStatus | undefined>>;
  dest: (item: SetItem, setId: string) => string;
};

const NOT_A_SET = new Set(['settings', 'module']);

export function watchTargets(input: WatchTargetInput): Map<string, string[]> {
  const open = input.tabs.filter(tab => !NOT_A_SET.has(tab.view.kind));
  const all = open.some(tab => tab.view.kind === 'repos');
  const wanted = new Set(open.map(tab => tab.setId));
  const targets = new Map<string, string[]>();
  for (const set of input.sets) {
    if (!all && !wanted.has(set.id)) continue;
    const roots = new Set(set.items.map(item => input.dest(item, set.id)).filter(path => input.local[path]?.repo));
    if (roots.size) targets.set(set.id, [...roots]);
  }
  return targets;
}
