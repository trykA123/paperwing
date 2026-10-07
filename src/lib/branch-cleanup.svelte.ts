import { api, type MergedBranches } from './api';
import {
  defaultSelection, deleteLocalAcross, localRows, selectableNames,
  type CleanupClient, type CleanupResult, type CleanupRow, type CleanupTarget, type LocalDelete,
} from './branch-cleanup';

export type CleanupLoad = { status: 'loading' } | { status: 'ready'; data: MergedBranches; local: CleanupRow[] } | { status: 'error'; message: string };
export type CleanupRepo = { target: CleanupTarget; load: CleanupLoad; localPicked: string[]; result: CleanupResult | null };
export type CleanupApi = CleanupClient & { mergedBranches: (path: string) => Promise<MergedBranches> };

const LOAD_CONCURRENCY = 4;

export class CleanupSession {
  repos = $state<CleanupRepo[]>([]);
  busy = $state(false);
  #disposed = false;

  constructor(targets: CleanupTarget[], private readonly client: CleanupApi = api) {
    this.repos = targets.map(target => ({ target, load: { status: 'loading' }, localPicked: [], result: null }));
  }

  loading = $derived(this.repos.some(repo => repo.load.status === 'loading'));
  localCount = $derived(this.repos.reduce((sum, repo) => sum + repo.localPicked.length, 0));

  async load(): Promise<void> {
    const queue = [...this.repos];
    const worker = async () => { for (let repo = queue.shift(); repo; repo = queue.shift()) await this.#read(repo.target.path); };
    await Promise.all(Array.from({ length: Math.min(LOAD_CONCURRENCY, queue.length) }, worker));
  }

  dispose() { this.#disposed = true; }

  #find(path: string): CleanupRepo {
    const repo = this.repos.find(entry => entry.target.path === path);
    if (!repo) throw new Error(`Unknown repository ${path}`);
    return repo;
  }

  async #read(path: string, keep = false): Promise<void> {
    const repo = this.#find(path);
    try {
      const data = await this.client.mergedBranches(path);
      if (this.#disposed) return;
      const local = localRows(data);
      repo.load = { status: 'ready', data, local };
      repo.localPicked = keep ? selectableNames(local, repo.localPicked) : defaultSelection(local);
    } catch (reason) {
      if (!this.#disposed) repo.load = { status: 'error', message: `Could not list branches in ${repo.target.name}: ${reason instanceof Error ? reason.message : String(reason)}` };
    }
  }

  toggle(path: string, name: string, on: boolean) {
    const repo = this.#find(path);
    repo.localPicked = on ? [...repo.localPicked.filter(entry => entry !== name), name] : repo.localPicked.filter(entry => entry !== name);
  }

  setAll(path: string, on: boolean) {
    const repo = this.#find(path);
    if (repo.load.status !== 'ready') return;
    repo.localPicked = on ? repo.load.local.filter(row => !row.blocked).map(row => row.name) : [];
  }

  #entries(): LocalDelete[] {
    return this.repos.flatMap(repo => {
      if (repo.load.status !== 'ready') return [];
      const names = selectableNames(repo.load.local, repo.localPicked);
      return names.length ? [{ target: repo.target, data: repo.load.data, rows: repo.load.local, names }] : [];
    });
  }

  async run(): Promise<CleanupResult[]> {
    if (this.busy) return [];
    this.busy = true;
    try {
      const entries = this.#entries();
      for (const repo of this.repos) repo.result = null;
      const results = await deleteLocalAcross(this.client, entries);
      for (const result of results) {
        this.#find(result.target.path).result = result;
        if (!this.#disposed) await this.#read(result.target.path, true);
      }
      return results;
    } finally { this.busy = false; }
  }
}
