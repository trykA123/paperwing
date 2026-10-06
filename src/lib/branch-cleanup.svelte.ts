import { api, type MergedBranches } from './api';
import {
  defaultSelection, deleteLocalAcross, deleteRemoteAcross, localRows, remoteRows, selectableNames,
  type CleanupClient, type CleanupResult, type CleanupRow, type CleanupTarget, type LocalDelete, type RemotePlan,
} from './branch-cleanup';

export type CleanupLoad = { status: 'loading' } | { status: 'ready'; data: MergedBranches; local: CleanupRow[]; remote: CleanupRow[] } | { status: 'error'; message: string };
export type CleanupRepo = { target: CleanupTarget; load: CleanupLoad; localPicked: string[]; remotePicked: string[]; result: CleanupResult | null; remoteResult: CleanupResult | null };
export type CleanupSide = 'local' | 'remote';
export type CleanupApi = CleanupClient & { mergedBranches: (path: string) => Promise<MergedBranches> };

const LOAD_CONCURRENCY = 4;

export class CleanupSession {
  repos = $state<CleanupRepo[]>([]);
  busy = $state(false);
  #disposed = false;

  constructor(targets: CleanupTarget[], private readonly client: CleanupApi = api) {
    this.repos = targets.map(target => ({ target, load: { status: 'loading' }, localPicked: [], remotePicked: [], result: null, remoteResult: null }));
  }

  loading = $derived(this.repos.some(repo => repo.load.status === 'loading'));
  localCount = $derived(this.repos.reduce((sum, repo) => sum + repo.localPicked.length, 0));
  remoteCount = $derived(this.repos.reduce((sum, repo) => sum + repo.remotePicked.length, 0));

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
      const local = localRows(data), remote = remoteRows(data);
      const [localKept, remoteKept] = [selectableNames(local, repo.localPicked), selectableNames(remote, repo.remotePicked)];
      repo.load = { status: 'ready', data, local, remote };
      repo.localPicked = keep ? localKept : defaultSelection(local);
      repo.remotePicked = keep ? remoteKept : [];
    } catch (reason) {
      if (!this.#disposed) repo.load = { status: 'error', message: `Could not list branches in ${repo.target.name}: ${reason instanceof Error ? reason.message : String(reason)}` };
    }
  }

  toggle(path: string, side: CleanupSide, name: string, on: boolean) {
    const repo = this.#find(path);
    const key = side === 'local' ? 'localPicked' : 'remotePicked';
    repo[key] = on ? [...repo[key].filter(entry => entry !== name), name] : repo[key].filter(entry => entry !== name);
  }

  setAll(path: string, side: CleanupSide, on: boolean) {
    const repo = this.#find(path);
    if (repo.load.status !== 'ready') return;
    const rows = side === 'local' ? repo.load.local : repo.load.remote;
    const names = on ? rows.filter(row => !row.blocked).map(row => row.name) : [];
    if (side === 'local') repo.localPicked = names; else repo.remotePicked = names;
  }

  #entries(side: CleanupSide): LocalDelete[] {
    return this.repos.flatMap(repo => {
      if (repo.load.status !== 'ready') return [];
      const rows = side === 'local' ? repo.load.local : repo.load.remote;
      const names = selectableNames(rows, side === 'local' ? repo.localPicked : repo.remotePicked);
      return names.length ? [{ target: repo.target, data: repo.load.data, rows, names }] : [];
    });
  }

  remotePlan(): RemotePlan[] {
    return this.#entries('remote').map(entry => ({ remote: entry.data.remote ?? '', repo: entry.target.name, names: entry.names }));
  }

  async run(side: CleanupSide): Promise<CleanupResult[]> {
    if (this.busy) return [];
    this.busy = true;
    try {
      const entries = this.#entries(side);
      const results = side === 'local' ? await deleteLocalAcross(this.client, entries) : await deleteRemoteAcross(this.client, entries);
      for (const result of results) {
        const repo = this.#find(result.target.path);
        if (side === 'local') repo.result = result; else repo.remoteResult = result;
        if (!this.#disposed) await this.#read(result.target.path, true);
      }
      return results;
    } finally { this.busy = false; }
  }
}
