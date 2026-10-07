import type { SetItem } from './api';
import { isCloned } from './formation';
import type { PullKey } from './pull-support';
import { app } from './state.svelte';

export type PullDialog = { kind: 'open'; item: SetItem } | { kind: 'bulk'; items: SetItem[] };

/** The branch a pull request would start from: a cloned repository that is on a branch, not a tag or a detached commit. */
export function pullKey(item: SetItem): PullKey | null {
  const path = app.dest(item);
  const local = app.local[path];
  return isCloned(local) && local?.branch ? { path, branch: local.branch } : null;
}

/** The listing carries no fork flag yet, so every repository is unknown and the backend decides. */
export const forkStatus = (_item: SetItem): 'fork' | 'not-fork' | 'unknown' => 'unknown';

export const pullable = (items: SetItem[]) => items.filter(item => pullKey(item));

class PullFlow {
  dialog = $state<PullDialog | null>(null);
  opener: HTMLElement | null = null;

  #open(dialog: PullDialog, opener?: Element | null) {
    if (this.dialog) return;
    const at = opener ?? document.activeElement;
    this.opener = at instanceof HTMLElement && !at.closest('[role=menu]') ? at : null;
    this.dialog = dialog;
  }

  openFor(item: SetItem, opener?: Element | null) { this.#open({ kind: 'open', item }, opener); }

  openBulk(items: SetItem[], opener?: Element | null) {
    const ready = pullable(items);
    if (!ready.length) { app.toast('No selected repository is on a branch', 'warn'); return; }
    this.#open({ kind: 'bulk', items: ready }, opener);
  }

  close() { this.dialog = null; }

  restoreFocus() {
    const opener = this.opener;
    if (opener?.isConnected) opener.focus();
  }
}

export const pullFlow = new PullFlow();
