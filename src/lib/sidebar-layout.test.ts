const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import { applySidebar, isNarrow, NARROW_BELOW, overlayAfterResize, overlayOnReveal, sidebarShown } from './sidebar-layout';

describe('sidebar layout', () => {
  test('the breakpoint is 1000 px', () => {
    expect(isNarrow(NARROW_BELOW - 1)).toBe(true);
    expect(isNarrow(NARROW_BELOW)).toBe(false);
  });

  test('narrow shows the overlay only; wide shows the docked choice', () => {
    expect(sidebarShown({ width: 390, docked: true, overlay: false })).toBe(false);
    expect(sidebarShown({ width: 390, docked: false, overlay: true })).toBe(true);
    expect(sidebarShown({ width: 1100, docked: true, overlay: true })).toBe(true);
    expect(sidebarShown({ width: 1100, docked: false, overlay: true })).toBe(false);
  });

  test('opening the overlay never changes the saved fold choice', () => {
    expect(applySidebar({ width: 390, docked: true, overlay: false }, true)).toEqual({ docked: true, overlay: true });
    expect(applySidebar({ width: 390, docked: false, overlay: true }, false)).toEqual({ docked: false, overlay: false });
  });

  test('a wide window saves the fold choice and drops any overlay', () => {
    expect(applySidebar({ width: 1440, docked: true, overlay: true }, false)).toEqual({ docked: false, overlay: false });
  });

  test('widening restores the docked sidebar unless the user folded it', () => {
    expect(sidebarShown({ width: 1440, docked: true, overlay: false })).toBe(true);
    expect(sidebarShown({ width: 1440, docked: false, overlay: false })).toBe(false);
  });

  test('growing past the breakpoint drops the overlay for good', () => {
    expect(overlayAfterResize(1440, true)).toBe(false);
    expect(overlayAfterResize(390, true)).toBe(true);
    expect(overlayAfterResize(390, false)).toBe(false);
  });

  test('page modules show their page, sidebar-only modules open the overlay', () => {
    expect(overlayOnReveal(true)).toBe(false);
    expect(overlayOnReveal(false)).toBe(true);
  });
});
