const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import { clearRange, selectionOffer, shiftRange } from './selection';

const ids = ['a', 'b', 'c', 'd', 'e'];

describe('shiftRange', () => {
  test('extends from the anchor and records what it switched on', () => {
    const on = new Set<string>();
    const first = shiftRange(ids, 'b', 'd', id => on.has(id), new Set());
    expect(first.on).toEqual(['b', 'c', 'd']);
    expect(first.added).toEqual(new Set(['b', 'c', 'd']));
  });

  test('reversing direction shrinks the range', () => {
    const on = new Set(['b', 'c', 'd']);
    const back = shiftRange(ids, 'b', 'c', id => on.has(id), new Set(['b', 'c', 'd']));
    expect(back.off).toEqual(['d']);
    expect(back.on).toEqual([]);
  });

  test('crossing the anchor flips the side and keeps rows that were on before', () => {
    const on = new Set(['a', 'b', 'c']);
    const added = new Set(['b', 'c']);
    const crossed = shiftRange(ids, 'b', 'a', id => on.has(id), added);
    expect(crossed.off).toEqual(['c']);
    expect(crossed.added).toEqual(new Set(['b']));
  });

  test('unknown ids change nothing', () => {
    expect(shiftRange(ids, 'x', 'a', () => false, new Set(['b']))).toEqual({ on: [], off: [], added: new Set(['b']) });
  });
});

describe('clearRange', () => {
  test('lists the rows between anchor and target in either direction', () => {
    expect(clearRange(ids, 'd', 'b')).toEqual(['b', 'c', 'd']);
  });
});

describe('selectionOffer', () => {
  const row = (on: boolean) => ({ on });
  test('offers all shown rows when the page is fully selected and more exist', () => {
    expect(selectionOffer([row(true), row(true)], [row(true), row(true), row(false)])).toBe(3);
  });
  test('no offer when the page is partial, everything is selected, or there is one page', () => {
    expect(selectionOffer([row(true), row(false)], [row(true), row(false), row(false)])).toBe(0);
    expect(selectionOffer([row(true)], [row(true), row(true)])).toBe(0);
    expect(selectionOffer([row(true), row(true)], [row(true), row(true)])).toBe(0);
  });
});
