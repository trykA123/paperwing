export type NoticeKind = 'info' | 'success' | 'warn' | 'error' | 'loading';
export type NoticeAction = { label: string; run: () => void };
export type NoticeItem = { id: number; msg: string; kind: NoticeKind; detail?: string; actions: NoticeAction[] };
export type NoticeOptions = { detail?: string; actions?: NoticeAction[] };
export type Scheduler = { set: (run: () => void, ms: number) => unknown; clear: (handle: unknown) => void };

export const NOTICE_MS: Record<NoticeKind, number | null> = { info: 4000, success: 4000, warn: 7000, error: null, loading: null };
export const NOTICE_LIMIT = 5;

const browserScheduler: Scheduler = { set: (run, ms) => setTimeout(run, ms), clear: handle => clearTimeout(handle as number) };

export class NotificationStore {
  items = $state<NoticeItem[]>([]);
  #timers = new Map<number, unknown>();
  #seq = 0;
  #clock: Scheduler;

  constructor(clock: Scheduler = browserScheduler) { this.#clock = clock; }

  notify(msg: string, kind: NoticeKind = 'info', options: NoticeOptions = {}) {
    const same = this.items.find(item => item.msg === msg && item.kind === kind);
    if (same) { this.#arm(same); return same.id; }
    const item: NoticeItem = { id: ++this.#seq, msg, kind, detail: options.detail, actions: options.actions ?? [] };
    const next = [...this.items, item];
    for (const dropped of next.slice(0, -NOTICE_LIMIT)) this.#cancel(dropped.id);
    this.items = next.slice(-NOTICE_LIMIT);
    this.#arm(item);
    return item.id;
  }

  update(id: number, patch: Partial<Omit<NoticeItem, 'id'>>) {
    const current = this.items.find(item => item.id === id);
    if (!current) return;
    const item = { ...current, ...patch };
    this.items = this.items.map(entry => (entry.id === id ? item : entry));
    this.#arm(item);
  }

  dismiss(id: number) {
    this.#cancel(id);
    this.items = this.items.filter(item => item.id !== id);
  }

  hold(id: number) { this.#cancel(id); }

  release(id: number) {
    const item = this.items.find(entry => entry.id === id);
    if (item) this.#arm(item);
  }

  act(id: number, action: NoticeAction) {
    action.run();
    this.dismiss(id);
  }

  #arm(item: NoticeItem) {
    this.#cancel(item.id);
    const ms = NOTICE_MS[item.kind];
    if (ms !== null) this.#timers.set(item.id, this.#clock.set(() => this.dismiss(item.id), ms));
  }

  #cancel(id: number) {
    this.#clock.clear(this.#timers.get(id));
    this.#timers.delete(id);
  }
}
