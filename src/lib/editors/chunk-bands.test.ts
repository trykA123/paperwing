const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import { bandOf, bandsIn, chunkBands, firstChunkFrom, kindOf, laneOf, rulerMarks, scrollForRuler, thumbOf, type BandChunk, type LineMetrics } from './chunk-bands';

const LINE = 20;
const lines: LineMetrics = { top: pos => pos * LINE, bottom: pos => (pos + 1) * LINE };
const chunk = (fromA: number, toA: number, fromB: number, toB: number): BandChunk => ({ fromA, toA, endA: Math.max(fromA, toA - 1), fromB, toB, endB: Math.max(fromB, toB - 1) });

describe('chunk kinds', () => {
  test('a chunk with no lines on the left is an addition', () => expect(kindOf(chunk(4, 4, 4, 7))).toBe('add'));
  test('a chunk with no lines on the right is a removal', () => expect(kindOf(chunk(4, 7, 4, 4))).toBe('rem'));
  test('a chunk with lines on both sides is a change', () => expect(kindOf(chunk(4, 6, 4, 9))).toBe('chg'));
  test('each kind has its own ruler lane, removed left of changed left of added', () => {
    expect([laneOf('rem'), laneOf('chg'), laneOf('add')]).toEqual([0, 1, 2]);
  });
});

describe('bands', () => {
  test('a change spans the taller side and neither side is a wedge', () => {
    expect(bandOf(chunk(2, 4, 2, 7), 0, lines, lines)).toEqual({ index: 0, kind: 'chg', top: 40, bottom: 7 * LINE, wedgeA: false, wedgeB: false });
  });

  test('an addition is a wedge on the left and spans the lines on the right', () => {
    const band = bandOf(chunk(5, 5, 5, 8), 3, lines, lines);
    expect(band).toMatchObject({ index: 3, kind: 'add', wedgeA: true, wedgeB: false, top: 5 * LINE, bottom: 8 * LINE });
  });

  test('a removal is a wedge on the right', () => {
    expect(bandOf(chunk(5, 8, 5, 5), 0, lines, lines)).toMatchObject({ kind: 'rem', wedgeA: false, wedgeB: true, top: 5 * LINE, bottom: 8 * LINE });
  });

  test('an empty chunk on both sides still has a visible height', () => {
    const band = bandOf(chunk(5, 5, 5, 5), 0, lines, lines);
    expect(band.bottom - band.top).toBeGreaterThan(0);
  });

  test('chunkBands keeps the chunk order and index', () => {
    const bands = chunkBands([chunk(1, 2, 1, 2), chunk(6, 6, 6, 8)], lines, lines);
    expect(bands.map(band => [band.index, band.kind])).toEqual([[0, 'chg'], [1, 'add']]);
  });

  test('the taller of two misaligned sides decides the bottom', () => {
    const shifted: LineMetrics = { top: pos => pos * LINE + 10, bottom: pos => (pos + 1) * LINE + 10 };
    const band = bandOf(chunk(2, 3, 2, 3), 0, lines, shifted);
    expect(band.top).toBe(40);
    expect(band.bottom).toBe(3 * LINE + 10);
  });
});

describe('windowing', () => {
  const chunks = Array.from({ length: 100 }, (_, index) => chunk(index * 10, index * 10 + 1, index * 10, index * 10 + 1));

  test('finds the first chunk that ends at or after a position', () => {
    expect(firstChunkFrom(chunks, 0)).toBe(0);
    expect(firstChunkFrom(chunks, 500)).toBe(50);
    expect(firstChunkFrom(chunks, 10_000)).toBe(100);
  });

  test('returns the chunks in the range with a pad on each side', () => {
    const indexes = bandsIn(chunks, { from: 500, to: 540 }, 2, lines, lines).map(band => band.index);
    expect(indexes).toEqual([48, 49, 50, 51, 52, 53, 54, 55, 56]);
  });

  test('stays inside the list at both ends', () => {
    expect(bandsIn(chunks, { from: 0, to: 15 }, 3, lines, lines).map(band => band.index)).toEqual([0, 1, 2, 3, 4]);
    expect(bandsIn(chunks, { from: 990, to: 5000 }, 3, lines, lines).at(-1)?.index).toBe(99);
  });
});

describe('ruler', () => {
  const bands = chunkBands([chunk(10, 12, 10, 12), chunk(40, 40, 40, 41)], lines, lines);

  test('scales bands onto the ruler and keeps a minimum mark height', () => {
    const marks = rulerMarks(bands, 100 * LINE, 200);
    expect(marks[0]).toMatchObject({ index: 0, kind: 'chg', lane: 1, top: 20, height: 4 });
    expect(marks[1]).toMatchObject({ kind: 'add', lane: 2, top: 80, height: 3 });
  });

  test('no content or no ruler draws nothing', () => {
    expect(rulerMarks(bands, 0, 200)).toEqual([]);
    expect(rulerMarks(bands, 2000, 0)).toEqual([]);
  });

  test('a mark at the very end stays inside the ruler', () => {
    const [mark] = rulerMarks([{ index: 0, kind: 'add', top: 1999, bottom: 2000, wedgeA: true, wedgeB: false }], 2000, 200);
    expect(mark!.top + mark!.height).toBeLessThanOrEqual(200);
  });

  test('the thumb shows the visible part and never shrinks below a grabbable size', () => {
    expect(thumbOf(0, 500, 2000, 400)).toEqual({ top: 0, height: 100 });
    expect(thumbOf(1500, 500, 2000, 400)).toEqual({ top: 300, height: 100 });
    expect(thumbOf(0, 50, 200_000, 400).height).toBe(20);
    expect(thumbOf(0, 500, 400, 400)).toEqual({ top: 0, height: 400 });
  });

  test('dragging maps a ruler position back to a scroll offset inside the content', () => {
    expect(scrollForRuler(200, 50, 500, 2000, 400)).toBe(750);
    expect(scrollForRuler(0, 50, 500, 2000, 400)).toBe(0);
    expect(scrollForRuler(400, 0, 500, 2000, 400)).toBe(1500);
  });
});
