import type { Workspace } from '../api';
import { OpenFolderStore, tauriTransport, type TempSet, type Transport } from '../open-folder.svelte';
import type { NoticeAction, NoticeKind } from '../notifications.svelte';
import { isTemporaryId, promoteTemporarySet } from '../temporary-set';
import { tabId, type ShellTab, type View } from '../workspace';

export type TemporaryHost = {
  ws: Workspace;
  tabs: ShellTab[];
  activeTabId: string;
  openView: (view: View, setId: string) => void;
  closeTab: (id: string) => Promise<void>;
  toast: (message: string, kind?: NoticeKind, action?: NoticeAction) => unknown;
  newId: () => string;
};

/** Sets opened from the file manager: listed and tabbed like sets, never written to settings until saved. */
export class TemporarySets {
  readonly store: OpenFolderStore;

  constructor(private readonly host: TemporaryHost, transport: Transport = tauriTransport) {
    this.store = new OpenFolderStore(transport, (message, kind, action) => host.toast(message, kind, action), {
      opened: set => host.openView({ kind: 'set' }, set.id),
      repository: set => this.showRepository(set),
      reveal: set => this.reveal(set),
    });
  }

  get sets(): TempSet[] { return this.store.sets; }

  find(id: string): TempSet | undefined { return this.store.sets.find(set => set.id === id); }

  start() { return this.store.start(); }

  stop() { this.store.stop(); }

  open(path: string) { return this.store.open(path); }

  save(id: string): string | null {
    const set = this.find(id);
    if (!set) return null;
    if (set.scanning) { this.host.toast('Wait for the scan to finish before saving this set.', 'warn'); return null; }
    const { set: saved, itemIds } = promoteTemporarySet($state.snapshot(set), this.host.newId(), this.host.newId);
    this.host.ws.sets.push(saved);
    this.retarget(id, saved.id, itemIds);
    this.store.dismiss(id);
    this.host.toast(`Saved "${saved.name}" as a set`, 'success');
    return saved.id;
  }

  async discard(id: string): Promise<void> {
    this.leave(id);
    for (const tab of [...this.host.tabs]) if (tab.setId === id && tab.view.kind !== 'settings') await this.host.closeTab(tab.id);
    this.store.dismiss(id);
  }

  /** Called after a tab closes: a temporary set nobody has open any more is thrown away. */
  releaseIfUnused(id: string): void {
    if (!isTemporaryId(id) || !this.find(id) || this.host.tabs.some(tab => tab.setId === id && tab.view.kind !== 'settings')) return;
    this.leave(id);
    this.store.dismiss(id);
  }

  private leave(id: string): void {
    const fallback = this.host.ws.sets[0].id;
    if (this.host.ws.activeSet === id) this.host.ws.activeSet = fallback;
    for (const tab of this.host.tabs) if (tab.setId === id && tab.view.kind === 'settings') tab.setId = fallback;
  }

  private retarget(from: string, to: string, itemIds: Map<string, string>): void {
    for (const tab of this.host.tabs) {
      if (tab.setId !== from) continue;
      const active = tab.id === this.host.activeTabId;
      if (tab.view.kind === 'item') tab.view = { ...tab.view, itemId: itemIds.get(tab.view.itemId) ?? tab.view.itemId };
      tab.setId = to;
      tab.id = tabId(tab.view, to);
      if (active) this.host.activeTabId = tab.id;
    }
    if (this.host.ws.activeSet === from) this.host.ws.activeSet = to;
  }

  private showRepository(set: TempSet): void {
    const item = set.items[0];
    if (!item) return;
    this.host.openView({ kind: 'item', itemId: item.id }, set.id);
    void this.host.closeTab(tabId({ kind: 'set' }, set.id));
  }

  private reveal(set: TempSet): void {
    const live = this.find(set.id);
    if (!live) return;
    const item = live.isRepository ? live.items[0] : undefined;
    this.host.openView(item ? { kind: 'item', itemId: item.id } : { kind: 'set' }, live.id);
  }
}
