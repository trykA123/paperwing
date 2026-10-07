import type { SetItem } from './api';
import { app } from './state.svelte';
import { withBusy, type Guarded } from './stash-switch';
import { refreshAfterTagChange, type TagTarget } from './tags-set';

export type TagDialog = { kind: 'create'; targets: TagTarget[] } | { kind: 'delete'; targets: TagTarget[]; tag: string };

const DEFAULT_REMOTE = 'origin';
export const tagTarget = (path: string, name: string, commit: string | null = null): TagTarget => ({ path, name, remote: DEFAULT_REMOTE, commit });
const fromItem = (item: SetItem) => tagTarget(app.dest(item), app.folderOf(item));
export const taggable = (items: SetItem[]) => items.filter(item => app.local[app.dest(item)]?.repo);

class TagFlow {
  dialog = $state<TagDialog | null>(null);
  /** Bumped after any tag change so open lists reload. */
  revision = $state(0);
  opener: HTMLElement | null = null;

  #open(dialog: TagDialog, opener?: Element | null) {
    if (this.dialog) return;
    const at = opener ?? document.activeElement;
    this.opener = at instanceof HTMLElement && !at.closest('[role=menu]') ? at : null;
    this.dialog = dialog;
  }

  #items(items: SetItem[]) {
    const targets = taggable(items).map(fromItem);
    if (!targets.length) app.toast('Clone the selected repositories first', 'warn');
    return targets;
  }

  openCreate(items: SetItem[], opener?: Element | null) { const targets = this.#items(items); if (targets.length) this.#open({ kind: 'create', targets }, opener); }
  openCreateFor(targets: TagTarget[], opener?: Element | null) { this.#open({ kind: 'create', targets }, opener); }
  openDelete(items: SetItem[], opener?: Element | null) { const targets = this.#items(items); if (targets.length) this.#open({ kind: 'delete', targets, tag: '' }, opener); }
  openDeleteFor(targets: TagTarget[], tag: string, opener?: Element | null) { this.#open({ kind: 'delete', targets, tag }, opener); }
  close() { this.dialog = null; }

  restoreFocus() {
    const opener = this.opener;
    if (opener?.isConnected) opener.focus();
    else document.querySelector<HTMLElement>('[data-tag-new]')?.focus();
  }

  changed(paths: string[]) {
    this.revision += 1;
    const urlOf = (path: string) => app.ws.sets.flatMap(set => set.items.filter(item => app.dest(item, set.id) === path).map(item => item.url));
    refreshAfterTagChange(paths, { urlsFor: urlOf, ensureRefs: (urls, force) => app.ensureRefs(urls, force), loadTree: (path, force) => app.loadTree(path, force) });
    void app.checkExists(paths);
  }

  /** Holds the Git lock for the app while `work` runs, then refreshes refs and trees. */
  async guarded<T>(paths: string[], work: () => Promise<T>): Promise<Guarded<T>> {
    const result = await withBusy(app, work);
    if (result.ran) this.changed(paths);
    return result;
  }
}

export const tagFlow = new TagFlow();
