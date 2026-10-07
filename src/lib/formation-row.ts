import type { LocalStatus, RefKind, SetItem } from './api';
import { isCloned, nextAction, syncView, type NextAction, type SyncView } from './formation';
import { pullKey } from './pull-flow.svelte';
import type { PullKey } from './pull-support';
import { rowFacts } from './row-actions';
import { isRemoteItem } from './repositories';
import { app } from './state.svelte';

export type RowModel = {
  item: SetItem; folder: string; sub: string; onDisk: boolean; problem: string | null;
  refType: RefKind; refLabel: string; refBad: boolean; refTitle: string; localNote: string | null;
  local: LocalStatus | undefined; sync: SyncView; next: NextAction | null; busy: string | null; fixed: boolean;
  selected: boolean; focused: boolean; canAct: boolean; pull: PullKey | null; remote: boolean; favorite: boolean;
};

const parentOf = (path: string) => path.replace(/[\\/][^\\/]*[\\/]?$/, '');

function refProblem(item: SetItem): { bad: boolean; title: string } {
  if (item.path) return { bad: false, title: 'The branch this folder has checked out' };
  const error = app.refs[item.url]?.error;
  if (app.refState(item) === 'missing') return { bad: true, title: `${item.ref.type === 'tag' ? 'Tag' : item.ref.type === 'branch' ? 'Branch' : 'Commit'} not found on the remote` };
  return { bad: false, title: error ?? 'Change branch, tag or commit' };
}

function problemOf(item: SetItem): string | null {
  if (item.on && app.view.kind !== 'repos' && app.hasClash(item)) return 'Same folder as another row';
  const job = app.jobs[item.id];
  return job?.phase === 'failed' ? job.msg : null;
}

/** A repository the listing has and no set holds. Its expected folder may already be on disk; then it is offered to a set, not cloned. */
function describeRemote(item: SetItem, state: RowState): RowModel {
  const local = app.local[app.dest(item)];
  const onDisk = isCloned(local);
  const label = local?.branchLabel ?? local?.branch ?? item.ref.name;
  return {
    item, folder: item.name, sub: item.org, onDisk, problem: null, refType: 'branch', refLabel: onDisk ? label : item.ref.name, refBad: false,
    refTitle: onDisk ? 'The branch this folder has checked out' : 'Default branch on the remote', localNote: null,
    local: onDisk ? local : undefined, sync: onDisk ? syncView(local) : { kind: 'missing' }, busy: app.rowBusy(item), fixed: true,
    next: onDisk ? { kind: 'adopt', label: 'Add to set', title: 'This folder is already on disk. Add it to a set to manage it.' } : { kind: 'clone', label: 'Clone', title: `Clone ${item.org}/${item.name}` },
    selected: state.selected ?? item.on, focused: state.focused, canAct: state.canAct, pull: onDisk ? pullKey(item) : null, remote: !onDisk, favorite: app.ws.stars.includes(item.repoId),
  };
}

type RowState = { focused: boolean; canAct: boolean; selected?: boolean };

export function describeRow(item: SetItem, state: RowState): RowModel {
  if (isRemoteItem(item)) return describeRemote(item, state);
  const facts = rowFacts(item);
  const ref = refProblem(item);
  const local = facts.local;
  const localLabel = local?.branchLabel ?? local?.branch ?? local?.tagLabel ?? local?.tag ?? local?.sha?.slice(0, 8) ?? '';
  return {
    item, folder: app.folderOf(item), onDisk: !item.path && !!app.exists[app.dest(item)], problem: problemOf(item),
    sub: item.path ? parentOf(item.path) : [item.folder ? item.name : '', item.org].filter(Boolean).join(' · '),
    refType: item.ref.type, refLabel: facts.refLabel, refBad: ref.bad, refTitle: ref.title,
    localNote: isCloned(local) && !facts.onRef && !item.path && localLabel ? `on ${localLabel}` : null,
    local, sync: syncView(local, app.statusFailures[app.dest(item)]), next: nextAction(facts), busy: app.rowBusy(item), fixed: !!item.path,
    selected: state.selected ?? item.on, focused: state.focused, canAct: state.canAct, pull: pullKey(item), remote: false, favorite: app.ws.stars.includes(item.repoId),
  };
}
