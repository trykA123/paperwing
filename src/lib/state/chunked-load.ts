export type ChunkedLoad<T, R> = {
  size: number;
  concurrency: number;
  load: (chunk: readonly T[]) => Promise<R>;
  publish: (result: R, chunk: readonly T[]) => void;
};

export type FailedChunk<T> = { chunk: readonly T[]; reason: unknown };

/** Loads items in chunks, a few at a time, publishing each chunk as it resolves. Returns the chunks that failed once every chunk has settled. */
export async function loadInChunks<T, R>(items: readonly T[], { size, concurrency, load, publish }: ChunkedLoad<T, R>) {
  const chunks: T[][] = [];
  for (let start = 0; start < items.length; start += size) chunks.push(items.slice(start, start + size));
  const failed: FailedChunk<T>[] = [];
  let next = 0;
  const worker = async () => {
    while (next < chunks.length) {
      const chunk = chunks[next++]!;
      try { publish(await load(chunk), chunk); }
      catch (reason) { failed.push({ chunk, reason }); }
    }
  };
  await Promise.all(Array.from({ length: Math.min(concurrency, chunks.length) }, worker));
  return failed;
}
