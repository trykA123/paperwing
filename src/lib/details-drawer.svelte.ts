import type { SetItem } from './api';
import type { GraphRow } from './history-graph';
import type { PullRequest } from './api';

export type DrawerTarget =
  | { kind: 'repository'; item: SetItem }
  | { kind: 'history'; path: string; name: string }
  | { kind: 'commit'; path: string; name: string; row: GraphRow }
  | { kind: 'pull'; name: string; pull: PullRequest }
  | { kind: 'stash'; path: string; name: string; oid: string };

/** The key that tells two drawers apart: opening another subject replaces the content, the same one keeps it. */
export function targetKey(target: DrawerTarget): string {
  switch (target.kind) {
    case 'repository': return `repository:${target.item.id}`;
    case 'history': return `history:${target.path}`;
    case 'commit': return `commit:${target.path}:${target.row.id}`;
    case 'pull': return `pull:${target.pull.url}`;
    case 'stash': return `stash:${target.path}:${target.oid}`;
  }
}

class DetailsDrawerStore {
  target = $state<DrawerTarget | null>(null);
  opener: HTMLElement | null = null;

  open(target: DrawerTarget, opener: Element | null = document.activeElement) {
    this.opener = opener instanceof HTMLElement ? opener : null;
    this.target = target;
  }

  openHistory(path: string, name: string, opener?: Element | null) { this.open({ kind: 'history', path, name }, opener); }
  close() { this.target = null; }
}

export const detailsDrawer = new DetailsDrawerStore();
