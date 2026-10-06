import { api, type RepositoryTree } from '../api';
import { ForegroundRequests } from './foreground-requests';

export class RepositoryTrees {
  trees = $state<Record<string, { data?: RepositoryTree; loading?: boolean; error?: string }>>({});
  private generations = new Map<string, number>();
  private scopes = new Map<string, string>();
  private bindings = new Map<string, string>();
  private requests = new ForegroundRequests();

  constructor(private scope: (path: string) => string = path => path) {}

  async readTree(path: string, signal?: AbortSignal) {
    await this.loadTree(path, false, signal);
    if (signal?.aborted) throw new DOMException('Metadata consumer closed', 'AbortError');
    const entry = this.trees[path];
    if (!entry?.data || entry.loading || entry.error) throw new Error(entry?.error ?? 'Repository metadata changed; reopen to refresh references');
    return entry.data;
  }

  loadTree(path: string, force = false, signal?: AbortSignal) {
    if (signal?.aborted) return Promise.resolve();
    const scope = this.scope(path);
    if (force || (this.scopes.has(path) && this.scopes.get(path) !== scope)) this.invalidate([path]);
    const key = JSON.stringify(['tree-v2', path, scope, this.generations.get(path) ?? 0]);
    const cached = this.trees[path]?.data;
    if (!force && cached?.identity && this.bindings.get(path) === JSON.stringify([key, cached.identity])) return Promise.resolve();
    this.scopes.set(path, scope);
    this.trees[path] = { ...this.trees[path], loading: true, error: undefined };
    const current = () => key === JSON.stringify(['tree-v2', path, this.scope(path), this.generations.get(path) ?? 0]);
    return this.requests.run(key, { signal,
      produce: () => api.repositoryTree(path),
      publish: data => { if (current()) {
        this.trees[path] = { data };
        if (data.identity) this.bindings.set(path, JSON.stringify([key, data.identity]));
      } },
      fail: error => { if (current()) this.trees[path] = { error: String(error) }; },
      settled: () => {
        const activeKey = JSON.stringify(['tree-v2', path, this.scope(path), this.generations.get(path) ?? 0]);
        if (this.trees[path]?.loading && !this.requests.has(activeKey)) delete this.trees[path];
      },
    });
  }

  invalidate(paths: string[]) {
    for (const path of new Set(paths)) {
      this.generations.set(path, (this.generations.get(path) ?? 0) + 1);
      delete this.trees[path];
      this.scopes.delete(path);
      this.bindings.delete(path);
    }
  }

  paths() { return [...new Set([...this.generations.keys(), ...this.scopes.keys()])]; }
}
