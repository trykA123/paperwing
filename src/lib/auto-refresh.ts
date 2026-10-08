export const REFRESH_WINDOW_MS = 300;

export type AutoRefreshHost = {
  refresh: (paths: string[]) => void;
  watch: (setId: string, roots: string[]) => Promise<unknown>;
  unwatch: (setId: string) => Promise<unknown>;
  windowMs?: number;
};

export type WatchTargets = ReadonlyMap<string, readonly string[]>;

const signatureOf = (roots: readonly string[]) => [...roots].sort().join('\n');

export class AutoRefresh {
  #host: AutoRefreshHost;
  #pending = new Set<string>();
  #timer: ReturnType<typeof setTimeout> | undefined;
  #applied = new Map<string, string>();
  #active = new Set<string>();
  #queue: Promise<void> = Promise.resolve();

  constructor(host: AutoRefreshHost) { this.#host = host; }

  get watching() { return this.#active.size; }

  changed(path: string) {
    this.#pending.add(path);
    this.#timer ??= setTimeout(() => this.#flush(), this.#host.windowMs ?? REFRESH_WINDOW_MS);
  }

  sync(targets: WatchTargets): Promise<void> {
    this.#queue = this.#queue.then(() => this.#reconcile(targets));
    return this.#queue;
  }

  #flush() {
    const paths = [...this.#pending];
    this.#pending.clear();
    this.#timer = undefined;
    if (paths.length) this.#host.refresh(paths);
  }

  async #reconcile(targets: WatchTargets) {
    for (const setId of [...this.#applied.keys()]) {
      if (!targets.get(setId)?.length) { this.#applied.delete(setId); this.#active.delete(setId); await this.#release(setId); }
    }
    for (const [setId, roots] of targets) {
      const signature = signatureOf(roots);
      if (!roots.length || this.#applied.get(setId) === signature) continue;
      this.#applied.set(setId, signature);
      await this.#start(setId, roots);
    }
  }

  async #start(setId: string, roots: readonly string[]) {
    await this.#host.watch(setId, [...roots]);
    this.#active.add(setId);
  }

  async #release(setId: string) {
    await this.#host.unwatch(setId);
  }
}
