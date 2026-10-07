import type { RepositoryHistory, RepositoryTree } from './api';

export type BaseChoices = { names: string[]; defaultBase: string };

const FALLBACKS = ['main', 'master'];

/** The remote's branches, and the one a pull request targets by default: the remote HEAD, then main, then master. */
export function baseChoices(tree: Pick<RepositoryTree, 'remotes'>): BaseChoices {
  const remote = tree.remotes.find(entry => entry.name === 'origin') ?? tree.remotes[0];
  if (!remote) return { names: [], defaultBase: '' };
  const strip = (name: string) => name.replace(/^refs\/remotes\//, '').replace(new RegExp(`^${remote.name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}/`), '');
  const head = remote.refs.find(ref => ref.symbolic && /\/HEAD$/.test(ref.name));
  const names = remote.refs.filter(ref => !ref.symbolic && !ref.name.endsWith('/HEAD')).map(ref => strip(ref.name)).sort();
  const target = head ? strip(head.symbolic) : '';
  const defaultBase = [target, ...FALLBACKS].find(name => name && names.includes(name)) ?? names[0] ?? '';
  return { names, defaultBase };
}

/** The pull request title starts as the newest commit subject on the branch, or the branch name. */
export function defaultTitle(history: Pick<RepositoryHistory, 'local'> | null, branch: string): string {
  return history?.local[0]?.subject?.trim() || branch;
}

export type Prepared = { names: string[]; base: string; title: string };
export type PrepareReads = { tree: (path: string) => Promise<RepositoryTree>; history: (path: string) => Promise<RepositoryHistory> };

/** Reads what a pull request form starts with; a failed read leaves that field for the user to fill. */
export async function preparePull(path: string, branch: string, reads: PrepareReads): Promise<Prepared> {
  const [tree, history] = await Promise.allSettled([reads.tree(path), reads.history(path)]);
  const { names, defaultBase } = tree.status === 'fulfilled' ? baseChoices(tree.value) : { names: [], defaultBase: '' };
  return { names, base: defaultBase, title: defaultTitle(history.status === 'fulfilled' ? history.value : null, branch) };
}

/** Runs `work` over every item with at most `limit` in flight; results keep the input order. */
export async function mapLimit<T, R>(items: readonly T[], limit: number, work: (item: T) => Promise<R>): Promise<R[]> {
  const results = new Array<R>(items.length);
  let next = 0;
  const lane = async () => { while (next < items.length) { const index = next++; results[index] = await work(items[index]!); } };
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, lane));
  return results;
}
