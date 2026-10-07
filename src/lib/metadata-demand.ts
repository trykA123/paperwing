export type Demand<T> = { request: (wanted: T[]) => void; release: () => void };

export function createDemand<T>(load: (wanted: T[], signal: AbortSignal) => unknown): Demand<T> {
  const consumer = new AbortController();
  return {
    request: wanted => { if (wanted.length && !consumer.signal.aborted) void load(wanted, consumer.signal); },
    release: () => consumer.abort(),
  };
}
