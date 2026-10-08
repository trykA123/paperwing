import type { SetItem } from './api';
import { choosePullKey } from './pull-key';
import type { PullKey } from './pull-support';
import { app } from './state.svelte';

export type PullDialog = { kind: 'open'; item: SetItem } | { kind: 'bulk'; items: SetItem[] };

export function pullKey(item: SetItem): PullKey | null {
  const path = app.dest(item);
  return choosePullKey(item, path, app.local[path], app.sources);
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
