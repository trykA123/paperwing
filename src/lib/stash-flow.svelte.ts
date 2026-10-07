import type { SetItem } from './api';
import { api } from './api';
import { confirm } from './confirm';
import { describeError } from './errors';
import type { MenuFacts } from './menu-reason';
import { withBusy, type Guarded, type SwitchTarget } from './stash-switch';
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

  /** Where focus returns when the dialog closes; menu items vanish, so they never count. */
  opener: HTMLElement | null = null;

  #open(dialog: StashDialog, opener?: Element | null) {
    if (this.dialog) return;
    const at = opener ?? document.activeElement;
    this.opener = at instanceof HTMLElement && !at.closest('[role=menu]') ? at : null;
    this.dialog = dialog;
  }

  openPush(items: SetItem[], opener?: Element | null) {
    const targets = stashable(items).map(target);
    if (!targets.length) { app.toast('No selected repository has changes to stash', 'warn'); return; }
    this.#open({ kind: 'push', targets }, opener);
  }

  openPushFor(targets: StashTarget[], opener?: Element | null) { this.#open({ kind: 'push', targets }, opener); }

  openSwitch(items: SetItem[], opener?: Element | null) {
    const targets = switchable(items).map(item => ({ ...target(item), branch: item.ref.name }));
    if (!targets.length) { app.toast('Every selected repository is already on its branch, or has no branch to switch to', 'warn'); return; }
    this.#open({ kind: 'switch', targets }, opener);
  }

  close() { this.dialog = null; }

  /** The opener may be disabled once the stash cleans the tree; then focus goes to its panel. */
  restoreFocus() {
    const opener = this.opener;
    if (!opener?.isConnected) return;
    opener.focus();
    setTimeout(() => {
      if (document.activeElement === document.body && opener.isConnected) opener.closest<HTMLElement>('[tabindex="-1"]')?.focus();
    }, 400);
  }
  changed() { this.revision += 1; }

  /** Holds the Git lock for the app while `work` runs, then refreshes status and open lists. */
  async guarded<T>(paths: string[], work: () => Promise<T>): Promise<Guarded<T>> {
    const result = await withBusy(app, work);
    if (result.ran) { this.changed(); void app.checkExists(paths); }
    return result;
  }

  /** Drop is always a separate, confirmed action that names the stash. */
  async drop(path: string, repo: string, oid: string, message: string): Promise<boolean> {
    const ok = await confirm(`Drop the stash "${message}" in ${repo}?\n\nThe stashed changes are removed from the stash list. Git keeps the objects for a while, but you cannot restore them from Skein.`,
      { title: 'Drop stash', kind: 'warning', okLabel: 'Drop stash', destructive: true });
    if (!ok) return false;
    try {
      const result = await this.guarded([path], () => api.stashDrop(path, oid));
      if (!result.ran) app.toast('A Git operation is already running', 'warn');
      return result.ran;
    } catch (reason) {
      app.toast(describeError(reason, `drop the stash in ${repo}`), 'error');
      return false;
    }
  }
}

export const stashFlow = new StashFlow();
