import { listen } from '@tauri-apps/api/event';
import { api, events } from './api';
import type { SearchCapabilities, SearchDone, SearchMatches, SearchRepoResult, SearchRequest, SearchSummary } from './api';
import { buildRows, type SearchGroup } from './search-results';
import { isSearchLimitError } from './search-request';

export type SearchHandlers = { matches: (event: SearchMatches) => void; repo: (event: SearchRepoResult) => void; done: (event: SearchDone) => void };
export type SearchTransport = {
  start: (request: SearchRequest) => Promise<number>;
  cancel: (id: number) => Promise<boolean>;
  cancelAll: () => Promise<number>;
  capabilities: () => Promise<SearchCapabilities>;
  subscribe: (handlers: SearchHandlers) => Promise<() => void>;
};
export type SearchStatus = 'idle' | 'starting' | 'running' | 'done' | 'cancelled' | 'failed';
type Early = { type: 'matches'; event: SearchMatches } | { type: 'repo'; event: SearchRepoResult } | { type: 'done'; event: SearchDone };

export const tauriSearchTransport: SearchTransport = {
  start: request => api.searchStart(request),
  cancel: id => api.searchCancel(id),
  cancelAll: () => api.searchCancelAll(),
  capabilities: () => api.searchCapabilities(),
  subscribe: async handlers => {
    const stops = await Promise.all([
      listen<SearchMatches>(events.searchMatches, event => handlers.matches(event.payload)),
      listen<SearchRepoResult>(events.searchRepo, event => handlers.repo(event.payload)),
      listen<SearchDone>(events.searchDone, event => handlers.done(event.payload)),
    ]);
    return () => stops.forEach(stop => stop());
  },
};

const EARLY_LIMIT = 5000;
const nextFrame = (run: () => void) => (typeof requestAnimationFrame === 'function' ? void requestAnimationFrame(run) : void setTimeout(run, 16));

/** One code search per tab: owns its job id, listens for the three events and ignores every other job's. */
export class SearchSession {
  status = $state<SearchStatus>('idle');
  error = $state<string | null>(null);
  summary = $state<SearchSummary | null>(null);
  perl = $state(true);
  matchCount = $state(0);
  reposDone = $state(0);
  reposTotal = $state(0);
  request: SearchRequest | null = null;
  #version = $state(0);
  rows = $derived.by(() => { void this.#version; return buildRows(this.#groups); });
  #groups: SearchGroup[] = [];
  #byRepo = new Map<string, SearchGroup>();
  #id: number | null = null;
  #early: Early[] = [];
  #stop: (() => void) | null = null;
  #disposed = false;
  #cancelWanted = false;
  #queued = false;

  constructor(private readonly transport: SearchTransport = tauriSearchTransport, private readonly schedule: (run: () => void) => void = nextFrame) {}

  get active(): boolean { return this.status === 'starting' || this.status === 'running'; }

  async loadCapabilities(): Promise<void> {
    try { this.perl = (await this.transport.capabilities()).perl; } catch { this.perl = false; }
  }

  async start(request: SearchRequest, names: Record<string, string>): Promise<void> {
    if (this.active || this.#disposed) return;
    this.#reset(request, names);
    this.status = 'starting';
    try {
      if (!this.#stop) {
        const stop = await this.transport.subscribe({
          matches: event => this.#receive({ type: 'matches', event }),
          repo: event => this.#receive({ type: 'repo', event }),
          done: event => this.#receive({ type: 'done', event }),
        });
        if (this.#disposed) { stop(); this.status = 'cancelled'; return; }
        this.#stop = stop;
      }
      const id = await this.transport.start(request);
      await this.#begin(id);
    } catch (reason) {
      this.#early = [];
      this.error = reason instanceof Error ? reason.message : String(reason);
      this.status = 'failed';
    }
  }

  async #begin(id: number): Promise<void> {
    this.#id = id;
    const early = this.#early.filter(entry => entry.event.id === id);
    this.#early = [];
    if (this.status === 'starting') this.status = 'running';
    for (const entry of early) this.#apply(entry);
    if (this.#disposed || this.#cancelWanted) await this.#cancelJob(id);
  }

  async cancel(): Promise<void> {
    if (!this.active) return;
    this.#cancelWanted = true;
    if (this.#id !== null) await this.#cancelJob(this.#id);
  }

  /** Frees the four search slots, then starts again; cancelled jobs release their slot a moment later. */
  async restartAfterCancellingAll(request: SearchRequest, names: Record<string, string>, attempts = 10): Promise<void> {
    await this.transport.cancelAll();
    for (let attempt = 0; attempt < attempts; attempt++) {
      await this.start(request, names);
      if (this.status !== 'failed' || !isSearchLimitError(this.error ?? '')) return;
      await new Promise(resolve => setTimeout(resolve, 150));
    }
  }

  async #cancelJob(id: number): Promise<void> {
    const found = await this.transport.cancel(id);
    if (!found && this.active) { this.status = 'cancelled'; this.#flush(); }
  }

  /** Cancels the running job and stops listening; call when the tab closes. */
  async dispose(): Promise<void> {
    this.#disposed = true;
    this.#stop?.();
    this.#stop = null;
    if (this.#id !== null && this.active) await this.transport.cancel(this.#id);
  }

  #reset(request: SearchRequest, names: Record<string, string>) {
    this.request = request;
    this.#groups = request.repos.map(target => ({ repo: target.path, name: names[target.path] ?? target.path, matches: [], status: null }));
    this.#byRepo = new Map(this.#groups.map(group => [group.repo, group]));
    this.#id = null;
    this.#early = [];
    this.#cancelWanted = false;
    this.error = null;
    this.summary = null;
    this.matchCount = 0;
    this.reposDone = 0;
    this.reposTotal = request.repos.length;
    this.#flush();
  }

  #receive(entry: Early) {
    if (this.#disposed) return;
    if (this.#id === null) {
      if (this.status === 'starting' && this.#early.length < EARLY_LIMIT) this.#early.push(entry);
      return;
    }
    if (entry.event.id === this.#id) this.#apply(entry);
  }

  #apply(entry: Early) {
    if (entry.type === 'matches') {
      const group = this.#byRepo.get(entry.event.repo);
      if (!group) return;
      for (const match of entry.event.matches) group.matches.push(match);
      this.matchCount += entry.event.matches.length;
      this.#queue();
    } else if (entry.type === 'repo') {
      const group = this.#byRepo.get(entry.event.repo);
      if (!group) return;
      if (!group.status) this.reposDone += 1;
      group.status = entry.event.status;
      this.#queue();
    } else {
      this.summary = entry.event.summary;
      this.status = entry.event.summary.cancelled ? 'cancelled' : 'done';
      this.#flush();
    }
  }

  #queue() {
    if (this.#queued) return;
    this.#queued = true;
    this.schedule(() => { this.#queued = false; this.#version += 1; });
  }

  #flush() { this.#version += 1; }
}
