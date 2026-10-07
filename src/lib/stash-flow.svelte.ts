import type { SetItem } from './api';
import { api } from './api';
import { confirm } from './confirm';
import { describeError } from './errors';
import type { MenuFacts } from './menu-reason';
import type { SwitchTarget } from './stash-switch';
import { app } from './state.svelte';

export type StashTarget = { path: string; name: string };
export type StashDialog = { kind: 'push'; targets: StashTarget[] } | { kind: 'switch'; targets: SwitchTarget[] };

const target = (item: SetItem): StashTarget => ({ path: app.dest(item), name: app.folderOf(item) });
const cloned = (item: SetItem) => !!app.local[app.dest(item)]?.repo;

/** Rows a stash can start from: cloned and with changes. */
export const stashable = (items: SetItem[]) => items.filter(item => cloned(item) && (app.local[app.dest(item)]?.dirty ?? 0) > 0);

/** Managed rows on a branch ref that the folder is not on yet; tags and commits cannot be switched to by branch name. */
export const switchable = (items: SetItem[]) => items.filter(item => !item.path && cloned(item) && item.ref.type === 'branch' && !app.onRef(item));

export function pathFacts(path: string): MenuFacts {
  const local = app.local[path];
  return {
    ready: app.ready, cloned: !!local?.repo, inPlace: false, idle: !(app.running || app.gitBusy), preparing: app.clonePreparing,
    behind: local?.behind ?? 0, ahead: local?.ahead ?? 0, dirty: local?.dirty ?? 0, onRef: true,
  };
}

class StashFlow {
  dialog = $state<StashDialog | null>(null);
  /** Bumped after any stash change so open lists reload. */
  revision = $state(0);

  openPush(items: SetItem[]) {
    const targets = stashable(items).map(target);
    if (!targets.length) { app.toast('No selected repository has changes to stash', 'warn'); return; }
    if (!this.dialog) this.dialog = { kind: 'push', targets };
  }

  openSwitch(items: SetItem[]) {
    const targets = switchable(items).map(item => ({ ...target(item), branch: item.ref.name }));
    if (!targets.length) { app.toast('Every selected repository is already on its branch, or has no branch to switch to', 'warn'); return; }
    if (!this.dialog) this.dialog = { kind: 'switch', targets };
  }

  close() { this.dialog = null; }
  changed() { this.revision += 1; }

  /** Holds the Git lock for the app while `work` runs, then refreshes status and open lists. */
  async guarded<T>(paths: string[], work: () => Promise<T>): Promise<T> {
    app.gitBusy = true;
    try { return await work(); }
    finally {
      app.gitBusy = false;
      this.changed();
      void app.checkExists(paths);
    }
  }

  /** Drop is always a separate, confirmed action that names the stash. */
  async drop(path: string, repo: string, oid: string, message: string): Promise<boolean> {
    const ok = await confirm(`Drop the stash "${message}" in ${repo}?\n\nThe stashed changes are removed from the stash list. Git keeps the objects for a while, but you cannot restore them from Skein.`,
      { title: 'Drop stash', kind: 'warning', okLabel: 'Drop stash', destructive: true });
    if (!ok) return false;
    try {
      await this.guarded([path], () => api.stashDrop(path, oid));
      return true;
    } catch (reason) {
      app.toast(describeError(reason, `drop the stash in ${repo}`), 'error');
      return false;
    }
  }
}

export const stashFlow = new StashFlow();
