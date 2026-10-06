export type HistoryTarget = { path: string; name: string };

class HistoryDrawerStore {
  target = $state<HistoryTarget | null>(null);
  opener: HTMLElement | null = null;

  open(target: HistoryTarget, opener: Element | null = document.activeElement) {
    this.opener = opener instanceof HTMLElement ? opener : null;
    this.target = target;
  }
  close() { this.target = null; }
}

export const historyDrawer = new HistoryDrawerStore();
