export type PrimaryFacts = { running: boolean; itemCount: number; cloneCount: number; fetchable: number };

/** One dark button in the set bar: the run's progress while it runs, else Clone when something is missing, else Fetch. */
export function headerPrimary(facts: PrimaryFacts): { clone: boolean; fetch: boolean; progress: boolean } {
  if (facts.itemCount === 0) return { clone: false, fetch: false, progress: false };
  if (facts.running) return { clone: false, fetch: false, progress: true };
  if (facts.cloneCount > 0) return { clone: true, fetch: false, progress: false };
  return { clone: false, fetch: facts.fetchable > 0, progress: false };
}
