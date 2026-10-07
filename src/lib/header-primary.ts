export type PrimaryFacts = { running: boolean; itemCount: number; cloneCount: number; rightVisible: boolean; fetchable: number };

/** The clone footer owns the primary while it is on screen; the header takes it only when the footer is absent. */
export function headerPrimary(facts: PrimaryFacts): { fetch: boolean; progress: boolean } {
  const footer = facts.rightVisible && facts.cloneCount > 0;
  if (footer || facts.itemCount === 0) return { fetch: false, progress: false };
  return { fetch: !facts.running && facts.fetchable > 0, progress: facts.running };
}
