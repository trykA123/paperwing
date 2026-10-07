import { HoverIntent, type Point, type Rect } from './flyout-hover';
import type { ProviderId } from './modules';

export type FlyoutOpen = { id: ProviderId; mode: 'hover' | 'pinned'; focus: boolean };

/** Which provider flyout is open and why: hover opens it after a short rest, a click, Enter, Space or an arrow key pins it. */
export class RailFlyoutState {
  open = $state<FlyoutOpen | null>(null);
  #hover = new HoverIntent(
    { set: (run, ms) => setTimeout(run, ms), clear: id => clearTimeout(id as ReturnType<typeof setTimeout>) },
    id => { this.open = { id: id as ProviderId, mode: 'hover', focus: false }; },
    () => { if (this.open?.mode === 'hover') this.open = null; },
  );

  enterTrigger(id: ProviderId) { this.#hover.enterTrigger(id, this.open?.id ?? null); }
  enterPanel() { this.#hover.enterPanel(); }
  leave(pointer: Point) { this.#hover.leave(pointer, this.open?.mode === 'hover'); }
  move(pointer: Point, flyout: Rect) { this.#hover.move(pointer, flyout); }

  show(id: ProviderId, focus: boolean) {
    this.#hover.cancel();
    this.open = { id, mode: 'pinned', focus };
  }

  toggle(id: ProviderId, focus: boolean) {
    if (this.open?.id === id && this.open.mode === 'pinned') this.close();
    else this.show(id, focus);
  }

  close() {
    this.#hover.cancel();
    this.open = null;
  }
}
