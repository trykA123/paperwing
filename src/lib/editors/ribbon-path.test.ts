const testModule = 'bun:test';
const { expect, test } = await import(testModule);
import type { Band } from './chunk-bands';
import { ribbonLabelY, ribbonPath } from './ribbon-path';

const band = (patch: Partial<Band>): Band => ({ index: 0, kind: 'chg', top: 100, bottom: 160, wedgeA: false, wedgeB: false, ...patch });

test('a change joins the same vertical span on both edges', () => {
  expect(ribbonPath(band({}), 56)).toBe('M0 100C28 100 28 100 56 100L56 160C28 160 28 160 0 160Z');
});

test('an addition narrows to a point on the left, in the middle of the span', () => {
  expect(ribbonPath(band({ kind: 'add', wedgeA: true }), 56)).toBe('M0 130C28 130 28 100 56 100L56 160C28 160 28 130 0 130Z');
});

test('a removal narrows to a point on the right', () => {
  expect(ribbonPath(band({ kind: 'rem', wedgeB: true }), 56)).toBe('M0 100C28 100 28 130 56 130L56 130C28 130 28 160 0 160Z');
});

test('the label sits in the middle of the span', () => {
  expect(ribbonLabelY(band({}))).toBe(130);
});
