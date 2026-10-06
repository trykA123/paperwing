export type HistoryTarget = { path: string; name: string };

class HistoryDrawerStore {
  target = $state<HistoryTarget | null>(null);

  open(target: HistoryTarget) { this.target = target; }
  close() { this.target = null; }
}

export const historyDrawer = new HistoryDrawerStore();
