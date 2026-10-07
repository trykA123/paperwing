import type { ProviderHost } from './modules';

export type HostCounts = { prs: number; actions: number; releases: number };

/** The source a repository belongs to: item ids read `<source id>:<path>`. */
export const sourceOfRepoId = (repoId: string, sourceIds: readonly string[]) => sourceIds.find(id => repoId.startsWith(`${id}:`));

/** Pull requests that need review, grouped by the host of their repository's source. */
export function awaitingByHost(awaiting: readonly { path: string }[], repoIdOfPath: ReadonlyMap<string, string>, hosts: readonly ProviderHost[]): Record<string, number> {
  const hostOfSource = new Map(hosts.flatMap(host => host.sourceIds.map(id => [id, host.host] as const)));
  const sourceIds = [...hostOfSource.keys()];
  const counts: Record<string, number> = Object.fromEntries(hosts.map(host => [host.host, 0]));
  for (const { path } of awaiting) {
    const source = sourceOfRepoId(repoIdOfPath.get(path) ?? '', sourceIds);
    const host = source ? hostOfSource.get(source) : undefined;
    if (host) counts[host] += 1;
  }
  return counts;
}

/** The host a repository lives on, from the source its id names. */
export function hostOfRepoId(repoId: string, hosts: readonly ProviderHost[]): string | undefined {
  return hosts.find(host => host.sourceIds.some(id => repoId.startsWith(`${id}:`)))?.host;
}
