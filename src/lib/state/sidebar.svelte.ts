import type { Workspace } from '../api';
import { applySidebar, isNarrow, overlayAfterResize, overlayOnReveal, sidebarShown } from '../sidebar-layout';

export type SidebarHost = { readonly ws: Workspace };

/** Whether the module sidebar is docked or floats over the page, and what the user chose in each case. */
export class SidebarLayout {
  private measured = $state(typeof window === 'undefined' ? 1440 : window.innerWidth);
  private open = $state(false);
  private readonly app!: SidebarHost;

  constructor(app: SidebarHost) { this.app = app; }

  get width() { return this.measured; }
  set width(value: number) {
    this.measured = value;
    this.open = overlayAfterResize(value, this.open);
  }

  narrow = $derived(isNarrow(this.width));
  overlay = $derived(this.narrow && this.open);
  shown = $derived(sidebarShown({ width: this.width, docked: this.app.ws.shell.sidebarVisible, overlay: this.open }));

  set(visible: boolean) {
    const next = applySidebar({ width: this.width, docked: this.app.ws.shell.sidebarVisible, overlay: this.open }, visible);
    this.app.ws.shell.sidebarVisible = next.docked;
    this.open = next.overlay;
  }

  toggle() { this.set(!this.shown); }

  close() { this.open = false; }

  /** Shows the sidebar of a module the user just picked. */
  reveal(ownsPage: boolean) { this.set(this.narrow ? overlayOnReveal(ownsPage) : true); }
}
