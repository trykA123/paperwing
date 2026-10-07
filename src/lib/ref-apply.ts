import type { Ref, SetItem } from './api';
import { plural } from './plural';
import { app } from './state.svelte';

/** Sets the branch, tag or commit a row asks for; for several rows, only those that have it on the remote. */
export function applyRef(chosen: readonly SetItem[], ref: Ref): void {
  if (chosen.length === 1) { app.setRef(chosen[0], ref); return; }
  const ok = chosen.filter(item => {
    const refs = app.refs[item.url];
    return !!refs && (ref.type === 'branch' ? refs.branches : refs.tags).includes(ref.name);
  });
  ok.forEach(item => app.setRef(item, ref));
  const miss = chosen.filter(item => !ok.includes(item)).map(item => item.name);
  const tail = miss.length ? ` (not in ${miss.slice(0, 4).join(', ')}${miss.length > 4 ? ` +${miss.length - 4} more` : ''})` : '';
  app.toast(`${app.refLabel({ ...chosen[0], ref })} applied to ${ok.length} of ${plural(chosen.length, 'repository', 'repositories')}${tail}`, miss.length ? 'warn' : 'success');
}
