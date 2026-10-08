import { OverviewRuler } from './overview-ruler';
import type { Surface, SurfaceInit } from './surface';

/** Adds the overview ruler to a mounted surface and keeps it in step with edits, resizes and the current change. */
export function withRuler(surface: Surface, init: Pick<SurfaceInit, 'host' | 'onJump'>): Surface {
  let current = 0;
  const ruler = new OverviewRuler({
    scroller: surface.scroller(), bands: () => surface.bands(), current: () => current, total: () => surface.chunks().length, onJump: init.onJump,
  });
  init.host.append(ruler.element);
  const stop = surface.onLayout(() => ruler.schedule());
  ruler.schedule();
  return {
    ...surface,
    markCurrent: index => { current = index; surface.markCurrent(index); ruler.schedule(); },
    destroy: () => { stop(); ruler.destroy(); surface.destroy(); },
  };
}
