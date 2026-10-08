export const UNKNOWN_COUNT = '–';

export const countLabel = (known: boolean, count: number) => (known ? String(count) : UNKNOWN_COUNT);

export const errorSummary = (errors: readonly string[] | undefined) => [...new Set(errors ?? [])].join(' · ');

export function newFailures(raised: Map<string, string>, errors: Record<string, string[] | undefined>, ids: readonly string[]) {
  const fresh: { id: string; message: string }[] = [];
  for (const id of ids) {
    const message = errorSummary(errors[id]);
    if (!message) raised.delete(id);
    else if (raised.get(id) !== message) { raised.set(id, message); fresh.push({ id, message }); }
  }
  return fresh;
}

export const DISABLED_LABEL = 'Disabled';
export const DISABLED_HINT = 'This source is disabled in Settings. Enable it to load repositories again.';

type SourceFlag = { id: string; enabled?: boolean };

export const isDisabled = (source: Pick<SourceFlag, 'enabled'> | undefined) => source?.enabled === false;

/** The source a repository belongs to: item and repository ids read `<source id>:<path>`. */
export const sourceOfId = <S extends SourceFlag>(repoId: string, sources: readonly S[]) => sources.find(source => repoId.startsWith(`${source.id}:`));

export const isRepoDisabled = (repoId: string, sources: readonly SourceFlag[]) => isDisabled(sourceOfId(repoId, sources));
