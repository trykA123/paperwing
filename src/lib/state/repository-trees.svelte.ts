import { api, type RepositoryTree } from '../api';

export class RepositoryTrees {
  trees = $state<Record<string, { data?: RepositoryTree; loading?: boolean; error?: string }>>({});
  #treeGeneration = new Map<string, number>();

  async loadTree(path: string, force = false) {
    if (!force && this.trees[path]) return;
    const generation = (this.#treeGeneration.get(path) ?? 0) + 1;
    this.#treeGeneration.set(path, generation);
    this.trees[path] = { ...this.trees[path], loading: true, error: undefined };
    try {
      const data = await api.repositoryTree(path);
      if (this.#treeGeneration.get(path) === generation) this.trees[path] = { data };
    } catch (error) {
      if (this.#treeGeneration.get(path) === generation) this.trees[path] = { error: String(error) };
    }
  }

  invalidate(paths: string[]) {
    for (const path of paths) {
      this.#treeGeneration.set(path, (this.#treeGeneration.get(path) ?? 0) + 1);
      delete this.trees[path];
    }
  }


  paths() { return [...this.#treeGeneration.keys()]; }
}
