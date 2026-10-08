import './test-support/svelte-loader.js';
const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import type { WindowHandle } from './fullscreen.svelte';

const { escapeLeaves, FullScreen } = await import('./fullscreen.svelte');

function fakeWindow(options: { maximized?: boolean; restoresMaximized?: boolean } = {}) {
  const calls: string[] = [];
  let maximized = options.maximized ?? false;
  const handle: WindowHandle = {
    isMaximized: async () => maximized,
    setFullscreen: async on => {
      calls.push(`fullscreen:${on}`);
      if (on) maximized = false;
      else if (options.restoresMaximized) maximized = options.maximized ?? false;
    },
    maximize: async () => { calls.push('maximize'); maximized = true; },
  };
  return { handle, calls };
}

function fixture(options: Parameters<typeof fakeWindow>[0] = {}) {
  const { handle, calls } = fakeWindow(options);
  const activated: string[] = [], failures: unknown[] = [];
  const screen = new FullScreen({ window: () => handle, activate: id => activated.push(id), fail: reason => failures.push(reason) });
  return { screen, calls, activated, failures };
}

describe('compare full screen', () => {
  test('entering asks the window for full screen and switches the layout', async () => {
    const { screen, calls } = fixture();
    await screen.enter();
    expect(screen.active).toBe(true);
    expect(calls).toEqual(['fullscreen:true']);
  });

  test('leaving restores a window that was not maximized without maximizing it', async () => {
    const { screen, calls } = fixture({ maximized: false, restoresMaximized: true });
    await screen.enter();
    await screen.exit();
    expect(screen.active).toBe(false);
    expect(calls).toEqual(['fullscreen:true', 'fullscreen:false']);
  });

  test('leaving puts a maximized window back to maximized when the system does not', async () => {
    const { screen, calls } = fixture({ maximized: true, restoresMaximized: false });
    await screen.enter();
    await screen.exit();
    expect(calls).toEqual(['fullscreen:true', 'fullscreen:false', 'maximize']);
  });

  test('leaving leaves a window alone when the system already restored it maximized', async () => {
    const { screen, calls } = fixture({ maximized: true, restoresMaximized: true });
    await screen.enter();
    await screen.exit();
    expect(calls).toEqual(['fullscreen:true', 'fullscreen:false']);
  });

  test('the browser build has no window and only switches the layout', async () => {
    const screen = new FullScreen({ window: () => null, activate() {}, fail() {} });
    await screen.enter();
    expect(screen.active).toBe(true);
    await screen.exit();
    expect(screen.active).toBe(false);
  });

  test('a failed window call undoes the layout change and reports', async () => {
    const calls: string[] = [], failures: unknown[] = [];
    const handle: WindowHandle = { isMaximized: async () => false, setFullscreen: async () => { throw new Error('denied'); }, maximize: async () => { calls.push('maximize'); } };
    const screen = new FullScreen({ window: () => handle, activate() {}, fail: reason => failures.push(reason) });
    await screen.toggle();
    expect(screen.active).toBe(false);
    expect(failures).toHaveLength(1);
  });

  test('opening a compare tab enters, and the tab before it is the way back', async () => {
    const { screen, calls, activated } = fixture();
    await screen.follow({ id: 'repos', compare: false });
    await screen.follow({ id: 'compare:1', compare: true });
    expect(screen.active).toBe(true);
    await screen.back();
    expect(screen.active).toBe(false);
    expect(activated).toEqual(['repos']);
    expect(calls).toEqual(['fullscreen:true', 'fullscreen:false']);
  });

  test('moving from a compare to another view ends the full screen the compare began', async () => {
    const { screen, calls } = fixture();
    await screen.follow({ id: 'repos', compare: false });
    await screen.follow({ id: 'compare:1', compare: true });
    await screen.follow({ id: 'repos', compare: false });
    expect(screen.active).toBe(false);
    expect(calls).toEqual(['fullscreen:true', 'fullscreen:false']);
  });

  test('moving between two compare tabs keeps the window as it is', async () => {
    const { screen, calls } = fixture();
    await screen.follow({ id: 'repos', compare: false });
    await screen.follow({ id: 'compare:1', compare: true });
    await screen.follow({ id: 'fileDiff:1', compare: true });
    expect(calls).toEqual(['fullscreen:true']);
  });

  test('F11 full screen outside a compare survives opening and leaving one', async () => {
    const { screen, calls, activated } = fixture();
    await screen.follow({ id: 'repos', compare: false });
    await screen.toggle();
    await screen.follow({ id: 'compare:1', compare: true });
    await screen.back();
    expect(screen.active).toBe(true);
    expect(activated).toEqual(['repos']);
    expect(calls).toEqual(['fullscreen:true']);
  });

  test('F11 inside a compare leaves full screen and stays on the compare', async () => {
    const { screen, calls, activated } = fixture();
    await screen.follow({ id: 'repos', compare: false });
    await screen.follow({ id: 'compare:1', compare: true });
    await screen.toggle();
    expect(screen.active).toBe(false);
    expect(activated).toEqual([]);
    expect(calls).toEqual(['fullscreen:true', 'fullscreen:false']);
  });

  test('toggling twice quickly runs the window calls in order', async () => {
    const { screen, calls } = fixture();
    const first = screen.toggle(), second = screen.toggle();
    await Promise.all([first, second]);
    expect(calls).toEqual(['fullscreen:true', 'fullscreen:false']);
    expect(screen.active).toBe(false);
  });
});

describe('Esc', () => {
  const field = (match: boolean) => ({ closest: () => (match ? {} : null) });
  const page = (open: boolean) => ({ querySelector: () => (open ? {} : null) });

  test('leaves full screen from the editor or the page', () => {
    expect(escapeLeaves(field(false), page(false))).toBe(true);
    expect(escapeLeaves(null, page(false))).toBe(true);
  });

  test('belongs to an open dialog, menu or search panel first', () => {
    expect(escapeLeaves(field(false), page(true))).toBe(false);
  });

  test('belongs to a text field first', () => {
    expect(escapeLeaves(field(true), page(false))).toBe(false);
  });
});
