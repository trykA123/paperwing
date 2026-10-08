export type ChangeKind = 'add' | 'rem' | 'chg';

export type BandChunk = { fromA: number; toA: number; endA: number; fromB: number; toB: number; endB: number };

/** Vertical position of the line block that holds `pos`, in the pixels of the scroll content. */
export type LineMetrics = { top(pos: number): number; bottom(pos: number): number };

/** One change as a vertical band. A wedge side has no lines and the band narrows to a point there. */
export type Band = { index: number; kind: ChangeKind; top: number; bottom: number; wedgeA: boolean; wedgeB: boolean };

export const MIN_BAND = 1;

export function kindOf(chunk: Pick<BandChunk, 'fromA' | 'toA' | 'fromB' | 'toB'>): ChangeKind {
  if (chunk.fromA === chunk.toA) return 'add';
  return chunk.fromB === chunk.toB ? 'rem' : 'chg';
}

export function bandOf(chunk: BandChunk, index: number, a: LineMetrics, b: LineMetrics): Band {
  const wedgeA = chunk.fromA === chunk.toA, wedgeB = chunk.fromB === chunk.toB;
  const topA = a.top(chunk.fromA), topB = b.top(chunk.fromB);
  const bottomA = wedgeA ? topA : a.bottom(chunk.endA), bottomB = wedgeB ? topB : b.bottom(chunk.endB);
  const top = Math.min(topA, topB);
  return { index, kind: kindOf(chunk), top, bottom: Math.max(bottomA, bottomB, top + MIN_BAND), wedgeA, wedgeB };
}

export function chunkBands(chunks: readonly BandChunk[], a: LineMetrics, b: LineMetrics): Band[] {
  return chunks.map((chunk, index) => bandOf(chunk, index, a, b));
}

/** Index of the first chunk whose A range ends at or after `pos`. */
export function firstChunkFrom(chunks: readonly BandChunk[], pos: number): number {
  let low = 0, high = chunks.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    if (chunks[middle]!.toA < pos) low = middle + 1; else high = middle;
  }
  return low;
}

/** The bands of the chunks that touch the A range `from` to `to`, plus `pad` chunks on each side. */
export function bandsIn(chunks: readonly BandChunk[], range: { from: number; to: number }, pad: number, a: LineMetrics, b: LineMetrics): Band[] {
  const first = Math.max(0, firstChunkFrom(chunks, range.from) - pad);
  const bands: Band[] = [];
  let beyond = 0;
  for (let index = first; index < chunks.length; index++) {
    const chunk = chunks[index]!;
    if (chunk.fromA > range.to && ++beyond > pad) break;
    bands.push(bandOf(chunk, index, a, b));
  }
  return bands;
}

export type RulerLane = 0 | 1 | 2;

/** Lanes from left to right: removed, changed, added. */
export function laneOf(kind: ChangeKind): RulerLane {
  return kind === 'rem' ? 0 : kind === 'add' ? 2 : 1;
}

export type RulerMark = { index: number; kind: ChangeKind; lane: RulerLane; top: number; height: number };

export const MIN_MARK = 3;

/** Maps content bands onto a ruler of `rulerHeight` px for content of `contentHeight` px. */
export function rulerMarks(bands: readonly Band[], contentHeight: number, rulerHeight: number): RulerMark[] {
  if (contentHeight <= 0 || rulerHeight <= 0) return [];
  const scale = rulerHeight / contentHeight;
  return bands.map(band => ({
    index: band.index, kind: band.kind, lane: laneOf(band.kind),
    top: Math.min(rulerHeight - MIN_MARK, Math.round(band.top * scale * 10) / 10),
    height: Math.max(MIN_MARK, Math.round((band.bottom - band.top) * scale * 10) / 10),
  }));
}

export type Thumb = { top: number; height: number };
export const MIN_THUMB = 20;

export function thumbOf(scrollTop: number, viewHeight: number, contentHeight: number, rulerHeight: number): Thumb {
  if (contentHeight <= 0) return { top: 0, height: rulerHeight };
  const scale = rulerHeight / contentHeight;
  const height = Math.min(rulerHeight, Math.max(MIN_THUMB, viewHeight * scale));
  return { top: Math.max(0, Math.min(rulerHeight - height, scrollTop * scale)), height };
}

/** The scroll offset that puts the middle of the thumb under `y` on the ruler. */
export function scrollForRuler(y: number, grab: number, viewHeight: number, contentHeight: number, rulerHeight: number): number {
  if (rulerHeight <= 0) return 0;
  const scale = rulerHeight / contentHeight;
  return Math.max(0, Math.min(contentHeight - viewHeight, (y - grab) / scale));
}
