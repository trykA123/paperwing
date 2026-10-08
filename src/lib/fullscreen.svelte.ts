export type WindowHandle = {
  isMaximized(): Promise<boolean>;
  setFullscreen(fullscreen: boolean): Promise<void>;
  maximize(): Promise<void>;
};

export type FullScreenHost = {
  window(): WindowHandle | null;
  activate(tabId: string): void;
  fail(reason: unknown): void;
};

export type FullScreenTab = { id: string; compare: boolean };

/** OS full screen for repository compares; the browser build has no window and only switches the layout. */
export class FullScreen {
  active = $state(false);
  private ownsWindow = false;
  private wasMaximized = false;
  private tabId = '';
  private onCompare = false;
  private returnTab = '';
  private chain: Promise<void> = Promise.resolve();
  private readonly host: FullScreenHost;

  constructor(host: FullScreenHost) { this.host = host; }

  private queue(task: (window: WindowHandle) => Promise<void>): Promise<void> {
    const run = async () => { const window = this.host.window(); if (window) await task(window); };
    const next = this.chain.then(run, run);
    this.chain = next.catch(() => {});
    return next;
  }

  async enter(): Promise<void> {
    if (this.active) return;
    this.active = true;
    try {
      await this.queue(async window => {
        this.wasMaximized = await window.isMaximized();
        await window.setFullscreen(true);
      });
    } catch (reason) { this.active = false; this.ownsWindow = false; throw reason; }
  }

  async exit(): Promise<void> {
    if (!this.active) return;
    this.active = false; this.ownsWindow = false;
    try {
      await this.queue(async window => {
        await window.setFullscreen(false);
        if (this.wasMaximized && !await window.isMaximized()) await window.maximize();
      });
    } catch (reason) { this.active = true; throw reason; }
  }

  toggle(): Promise<void> {
    if (this.active) return this.report(this.exit());
    this.ownsWindow = this.onCompare;
    return this.report(this.enter());
  }

  private async report(task: Promise<void>): Promise<void> {
    try { await task; } catch (reason) { this.host.fail(reason); }
  }

  /** Called whenever the active tab changes; opening a compare tab enters full screen, leaving one for another view ends it. */
  follow(tab: FullScreenTab): Promise<void> {
    if (tab.id === this.tabId) return Promise.resolve();
    this.tabId = tab.id; this.onCompare = tab.compare;
    if (!tab.compare) {
      this.returnTab = tab.id;
      return this.active && this.ownsWindow ? this.report(this.exit()) : Promise.resolve();
    }
    if (this.active) return Promise.resolve();
    this.ownsWindow = true;
    return this.report(this.enter());
  }

  async back(): Promise<void> {
    const target = this.returnTab;
    await this.report(this.ownsWindow ? this.exit() : Promise.resolve());
    this.host.activate(target);
  }
}

const OVERLAYS = 'dialog[open], .cm-search, .compare-menu, .confirm-bar, .compare-tab:not([hidden]) [aria-expanded="true"]';

/** Esc leaves full screen unless it belongs to an open dialog, menu, search panel or text field. */
export function escapeLeaves(target: { closest(selector: string): unknown } | null, page: { querySelector(selector: string): unknown }): boolean {
  if (page.querySelector(OVERLAYS)) return false;
  return !target?.closest('input, textarea, select');
}
