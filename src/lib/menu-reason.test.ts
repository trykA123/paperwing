const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import { disabledReason, type MenuFacts } from './menu-reason';

const ok: MenuFacts = { ready: true, cloned: true, inPlace: false, idle: true, preparing: false, behind: 2, ahead: 1, dirty: 3, onRef: false, hasBranch: true };

describe('disabledReason', () => {
  test('returns null when every need is met', () => {
    expect(disabledReason(['cloned', 'managed', 'behind', 'unpushed', 'offRef', 'dirty'], ok)).toBeNull();
  });
  test('not ready, preparing and busy outrank row facts', () => {
    expect(disabledReason(['cloned'], { ...ok, ready: false, cloned: false })).toBe('Skein is still loading');
    expect(disabledReason(['cloned'], { ...ok, preparing: true })).toBe('Preparing a clone');
    expect(disabledReason(['cloned'], { ...ok, idle: false })).toBe('A Git operation is already running');
  });
  test('folders opened in place', () => {
    expect(disabledReason(['managed'], { ...ok, inPlace: true })).toBe('Not available for folders opened in place yet');
    expect(disabledReason(['cloned'], { ...ok, inPlace: true })).toBeNull();
  });
  test('not cloned', () => {
    expect(disabledReason(['cloned'], { ...ok, cloned: false })).toBe('Clone the repository first');
  });
  test('state needs', () => {
    expect(disabledReason(['behind'], { ...ok, behind: 0 })).toBe('Not behind its upstream');
    expect(disabledReason(['unpushed'], { ...ok, ahead: 0 })).toBe('Nothing to push');
    expect(disabledReason(['offRef'], { ...ok, onRef: true })).toBe('Already on the set’s branch');
    expect(disabledReason(['dirty'], { ...ok, dirty: 0 })).toBe('No uncommitted changes');
    expect(disabledReason(['branch'], { ...ok, hasBranch: false })).toBe('Not on a branch');
  });
  test('an action with no needs is never disabled by row facts', () => {
    expect(disabledReason([], { ...ok, cloned: false, inPlace: true })).toBeNull();
  });
});
