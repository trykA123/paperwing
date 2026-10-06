import type { SetItem } from './api';
import type { NextActionKind, RowFacts } from './formation';
import { historyDrawer } from './history-drawer.svelte';
import { app } from './state.svelte';

export const rowFacts = (item: SetItem): RowFacts => ({
  local: app.local[app.dest(item)], onRef: app.onRef(item), refLabel: app.refLabel(item), fixedFolder: !!item.path, refMissing: app.refState(item) === 'missing',
});

/** A repository the set wants but the disk does not have yet. Before the first status check it counts only when the root is unusable. */
export function needsClone(item: SetItem): boolean {
  if (item.path) return false;
  const local = app.local[app.dest(item)];
  return local ? !local.exists : !app.rootSupport.valid;
}

export const pushTarget = (item: SetItem) => ({ path: app.dest(item), name: app.folderOf(item) });

export function runNextAction(item: SetItem, kind: NextActionKind): void {
  switch (kind) {
    case 'clone': void app.startClone([item]); break;
    case 'commit': app.openGitDialog('commit', item); break;
    case 'switch': void app.startClone([item], 'switch'); break;
    case 'pull': void app.startClone([item], 'pull'); break;
    case 'push': void app.pushRepos([pushTarget(item)]); break;
    case 'diverged': openHistory(item); break;
  }
}

export function openHistory(item: SetItem, opener?: Element | null): void {
  historyDrawer.open({ path: app.dest(item), name: app.folderOf(item) }, opener);
}
