import { openUrl } from '@tauri-apps/plugin-opener';
import type { CreatedPullRequest, OpenPullRequest, PullRequest, PullsError } from './api';
import { api } from './api';
import { keyOf, rateLimitText, readPullsError, type PullKey } from './pull-support';

export type PullEntry =
  | { status: 'loading' }
  | { status: 'ready'; pull: PullRequest | null }
  | { status: 'failed'; message: string };
export type RateLimit = { resetAt: string; message: string };
export type PullsApi = {
  pullForBranch: (path: string, branch: string) => Promise<PullRequest | null>;
  openPullRequest: (path: string, request: OpenPullRequest) => Promise<CreatedPullRequest>;
};
/** Selecting a whole set must not request the whole set; only this many selected rows load ahead of scrolling. */
export const PIN_LIMIT = 50;

type RateLimitStop = { stop: Extract<PullsError, { kind: 'rateLimited' }> };

export type PullLoaderOptions = { concurrency?: number; settleMs?: number; now?: () => number; onRateLimit?: (limit: RateLimit) => void };

/** Loads the pull request of each repository's current branch: lazily, four at a time, once per session, never in a retry loop. */
export class PullLoader {
  entries = $state<Record<string, PullEntry>>({});
  limit = $state<RateLimit | null>(null);

  #api: PullsApi;
  #concurrency: number;
  #settleMs: number;
  #now: () => number;
  #onRateLimit: ((limit: RateLimit) => void) | undefined;
  #wanted = new Map<string, { key: PullKey; count: number }>();
  #pinned = new Map<string, PullKey>();
  #ensured = new Map<string, PullKey>();
  #queue: PullKey[] = [];
  #active = 0;
  #generation = new Map<string, number>();
  #inflight = new Set<string>();
  #again = new Set<string>();
  #timer: ReturnType<typeof setTimeout> | undefined;
  #waiters: { keys: PullKey[]; done: () => void }[] = [];

  constructor(pullsApi: PullsApi, options: PullLoaderOptions = {}) {
    this.#api = pullsApi;
    this.#concurrency = options.concurrency ?? 4;
    this.#settleMs = options.settleMs ?? 0;
    this.#now = options.now ?? Date.now;
    this.#onRateLimit = options.onRateLimit;
  }

  /** Rows whose pull request is known to await a review; the rail badge can read this. */
  awaitingReview = $derived(Object.values(this.entries).filter(entry => entry.status === 'ready' && entry.pull?.state === 'open' && entry.pull.reviewState === 'reviewRequired').length);

  loading = $derived(Object.values(this.entries).filter(entry => entry.status === 'loading').length);

  entry(key: PullKey): PullEntry | undefined { return this.entries[keyOf(key)]; }

  /** A row on screen asks for its pull request; call the returned function when it leaves the screen. */
  want(key: PullKey): () => void {
    const id = keyOf(key);
    const known = this.#wanted.get(id);
    if (known) known.count += 1; else this.#wanted.set(id, { key, count: 1 });
    this.#enqueue(key);
    return () => {
      const current = this.#wanted.get(id);
      if (!current) return;
      current.count -= 1;
      if (current.count <= 0) this.#wanted.delete(id);
    };
  }

  /** Selected rows load even when they are scrolled out of view. */
  pin(keys: readonly PullKey[]) {
    const chosen = keys.slice(0, PIN_LIMIT);
    this.#pinned = new Map(chosen.map(key => [keyOf(key), key]));
    for (const key of chosen) this.#enqueue(key);
  }

  /** Loads every key through the same queue and resolves when each is answered, failed, or left unasked by a rate limit. */
  ensure(keys: readonly PullKey[]): Promise<void> {
    for (const key of keys) { this.#ensured.set(keyOf(key), key); this.#enqueue(key); }
    return new Promise(resolve => {
      this.#waiters.push({ keys: [...keys], done: () => { for (const key of keys) this.#ensured.delete(keyOf(key)); resolve(); } });
      this.#notify();
    });
  }

  /** Forgets the answers for these rows (or every row on screen and selected) and asks again. */
  refresh(keys?: readonly PullKey[]) {
    const targets = keys ?? [...this.#wanted.values()].map(entry => entry.key).concat([...this.#pinned.values()]);
    for (const key of targets) {
      const id = keyOf(key);
      this.#generation.set(id, (this.#generation.get(id) ?? 0) + 1);
      if (this.#inflight.has(id)) { this.#again.add(id); continue; }
      delete this.entries[id];
    }
    for (const key of targets) this.#enqueue(key);
  }

  dispose() {
    clearTimeout(this.#timer);
    this.#timer = undefined;
  }

  #limited(): boolean {
    if (this.limit && Date.parse(this.limit.resetAt) <= this.#now()) this.#resume();
    return this.limit !== null;
  }

  /** After the reset time every row that is still wanted asks again, without a new want call. */
  #resume() {
    this.dispose();
    this.limit = null;
    const keys = [...this.#wanted.values()].map(entry => entry.key).concat([...this.#pinned.values()], [...this.#ensured.values()]);
    for (const key of keys) this.#enqueue(key);
  }

  #interested(key: PullKey) {
    const id = keyOf(key);
    return this.#wanted.has(id) || this.#pinned.has(id) || this.#ensured.has(id);
  }

  #enqueue(key: PullKey) {
    const id = keyOf(key);
    if (this.entries[id] || this.#limited()) return;
    this.entries[id] = { status: 'loading' };
    this.#queue.push(key);
    this.#pump();
  }

  #pump() {
    while (this.#active < this.#concurrency && this.#queue.length) {
      const key = this.#queue.shift()!;
      this.#active += 1;
      void this.#run(key).finally(() => { this.#active -= 1; this.#pump(); this.#notify(); });
    }
  }

  #notify() {
    const answered = (key: PullKey) => { const entry = this.entries[keyOf(key)]; return entry ? entry.status !== 'loading' : this.limit !== null; };
    const ready = this.#waiters.filter(waiter => waiter.keys.every(answered));
    this.#waiters = this.#waiters.filter(waiter => !ready.includes(waiter));
    for (const waiter of ready) waiter.done();
  }

  async #run(key: PullKey) {
    const id = keyOf(key);
    if (this.#settleMs) await new Promise(resolve => setTimeout(resolve, this.#settleMs));
    if (!this.#interested(key) || this.#limited()) { delete this.entries[id]; return; }
    const generation = this.#generation.get(id) ?? 0;
    this.#inflight.add(id);
    let outcome: PullEntry | RateLimitStop;
    try {
      outcome = { status: 'ready', pull: await this.#api.pullForBranch(key.path, key.branch) };
    } catch (reason) {
      const error = readPullsError(reason);
      outcome = error.kind === 'rateLimited' ? { stop: error } : { status: 'failed', message: error.message };
    }
    this.#inflight.delete(id);
    if ((this.#generation.get(id) ?? 0) !== generation) {
      delete this.entries[id];
      if (this.#again.delete(id)) this.#enqueue(key);
    } else if ('stop' in outcome) { delete this.entries[id]; this.#stop(outcome.stop); }
    else this.entries[id] = outcome;
  }

  #stop(error: Extract<PullsError, { kind: 'rateLimited' }>) {
    const first = this.limit === null;
    this.limit = { resetAt: error.resetAt, message: error.message };
    for (const key of this.#queue) delete this.entries[keyOf(key)];
    this.#queue = [];
    clearTimeout(this.#timer);
    const wait = Math.min(Math.max(0, Date.parse(error.resetAt) - this.#now()), 2 ** 31 - 1);
    this.#timer = Number.isNaN(wait) ? undefined : setTimeout(() => this.#resume(), wait);
    if (first) this.#onRateLimit?.(this.limit);
  }
}

export const pulls = new PullLoader(api, {
  settleMs: 150,
  onRateLimit: limit => import('./state.svelte').then(({ app }) => app.toast(rateLimitText(limit.resetAt), 'warn')),
});

export const openPull = (url: string) => openUrl(url);
export const createPull = (path: string, request: OpenPullRequest) => api.openPullRequest(path, request);
