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
  #queue: PullKey[] = [];
  #active = 0;

  constructor(pullsApi: PullsApi, options: PullLoaderOptions = {}) {
    this.#api = pullsApi;
    this.#concurrency = options.concurrency ?? 4;
    this.#settleMs = options.settleMs ?? 0;
    this.#now = options.now ?? Date.now;
    this.#onRateLimit = options.onRateLimit;
  }

  /** Rows whose pull request is known to await a review; the rail badge can read this. */
  awaitingReview = $derived(Object.values(this.entries).filter(entry => entry.status === 'ready' && entry.pull?.state === 'open' && entry.pull.reviewState === 'reviewRequired').length);

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

  /** Forgets the answers for these rows (or every row on screen and selected) and asks again. */
  refresh(keys?: readonly PullKey[]) {
    const targets = keys ?? [...this.#wanted.values()].map(entry => entry.key).concat([...this.#pinned.values()]);
    for (const key of targets) delete this.entries[keyOf(key)];
    for (const key of targets) this.#enqueue(key);
  }

  #limited(): boolean {
    if (this.limit && Date.parse(this.limit.resetAt) <= this.#now()) this.limit = null;
    return this.limit !== null;
  }

  #interested(key: PullKey) {
    const id = keyOf(key);
    return this.#wanted.has(id) || this.#pinned.has(id);
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
      void this.#run(key).finally(() => { this.#active -= 1; this.#pump(); });
    }
  }

  async #run(key: PullKey) {
    const id = keyOf(key);
    if (this.#settleMs) await new Promise(resolve => setTimeout(resolve, this.#settleMs));
    if (!this.#interested(key) || this.#limited()) { delete this.entries[id]; return; }
    try {
      this.entries[id] = { status: 'ready', pull: await this.#api.pullForBranch(key.path, key.branch) };
    } catch (reason) {
      const error = readPullsError(reason);
      if (error.kind === 'rateLimited') { delete this.entries[id]; this.#stop(error); }
      else this.entries[id] = { status: 'failed', message: error.message };
    }
  }

  #stop(error: Extract<PullsError, { kind: 'rateLimited' }>) {
    const first = this.limit === null;
    this.limit = { resetAt: error.resetAt, message: error.message };
    for (const key of this.#queue) delete this.entries[keyOf(key)];
    this.#queue = [];
    if (first) this.#onRateLimit?.(this.limit);
  }
}

export const pulls = new PullLoader(api, {
  settleMs: 150,
  onRateLimit: limit => import('./state.svelte').then(({ app }) => app.toast(rateLimitText(limit.resetAt), 'warn')),
});

export const openPull = (url: string) => openUrl(url);
export const createPull = (path: string, request: OpenPullRequest) => api.openPullRequest(path, request);
