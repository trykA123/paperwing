export type ChunkedLoad<T, R> = {
  size: number;
  concurrency: number;
  load: (chunk: readonly T[]) => Promise<R>;
  publish: (result: R, chunk: readonly T[]) => void;
};

/** Loads items in chunks, a few at a time, publishing each chunk as it resolves. Rethrows the first failure once every chunk has settled. */
export async function loadInChunks<T, R>(items: readonly T[], { size, concurrency, load, publish }: ChunkedLoad<T, R>) {
  const chunks: T[][] = [];
  for (let start = 0; start < items.length; start += size) chunks.push(items.slice(start, start + size));
  let next = 0;
  let failure: { reason: unknown } | undefined;
  const worker = async () => {
    while (next < chunks.length) {
      const chunk = chunks[next++]!;
      try { publish(await load(chunk), chunk); }
      catch (reason) { failure ??= { reason }; }
    }
  };
  await Promise.all(Array.from({ length: Math.min(concurrency, chunks.length) }, worker));
  if (failure) throw failure.reason;
}
