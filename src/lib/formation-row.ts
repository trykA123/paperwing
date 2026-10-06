import type { LocalStatus, RefKind, SetItem } from './api';
import { isCloned, nextAction, syncView, type NextAction, type SyncView } from './formation';
import { rowFacts } from './row-actions';
import { app } from './state.svelte';

export type RowModel = {
  item: SetItem; folder: string; sub: string; onDisk: boolean; problem: string | null;
  refType: RefKind; refLabel: string; refBad: boolean; refTitle: string; localNote: string | null;
  local: LocalStatus | undefined; sync: SyncView; next: NextAction | null; busy: string | null; fixed: boolean;
  selected: boolean; focused: boolean; canAct: boolean;
};

const parentOf = (path: string) => path.replace(/[\\/][^\\/]*[\\/]?$/, '');

function refProblem(item: SetItem): { bad: boolean; title: string } {
  if (item.path) return { bad: false, title: 'The branch this folder has checked out' };
  const error = app.refs[item.url]?.error;
  if (app.refState(item) === 'missing') return { bad: true, title: `${item.ref.type === 'tag' ? 'Tag' : item.ref.type === 'branch' ? 'Branch' : 'Commit'} not found on the remote` };
  return { bad: false, title: error ?? 'Change branch, tag or commit' };
}

function problemOf(item: SetItem): string | null {
  if (item.on && app.hasClash(item)) return 'Same folder as another row';
  const job = app.jobs[item.id];
  return job?.phase === 'failed' ? job.msg : null;
}

export function describeRow(item: SetItem, state: { focused: boolean; canAct: boolean }): RowModel {
  const facts = rowFacts(item);
  const ref = refProblem(item);
  const local = facts.local;
  const localLabel = local?.branchLabel ?? local?.branch ?? local?.tagLabel ?? local?.tag ?? local?.sha?.slice(0, 8) ?? '';
  return {
    item, folder: app.folderOf(item), onDisk: !item.path && !!app.exists[app.dest(item)], problem: problemOf(item),
    sub: item.path ? parentOf(item.path) : [item.folder ? item.name : '', item.org].filter(Boolean).join(' · '),
    refType: item.ref.type, refLabel: facts.refLabel, refBad: ref.bad, refTitle: ref.title,
    localNote: isCloned(local) && !facts.onRef && !item.path && localLabel ? `on ${localLabel}` : null,
    local, sync: syncView(local), next: nextAction(facts), busy: app.rowBusy(item), fixed: !!item.path,
    selected: item.on, focused: state.focused, canAct: state.canAct,
  };
}
