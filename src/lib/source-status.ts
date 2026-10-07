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
