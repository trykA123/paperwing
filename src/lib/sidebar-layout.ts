export const NARROW_BELOW = 1000;

export const isNarrow = (width: number) => width < NARROW_BELOW;

export type SidebarFacts = { width: number; docked: boolean; overlay: boolean };

/** Below the breakpoint the sidebar is an overlay the user opens; above it, the docked sidebar follows the user's own fold choice. */
export const sidebarShown = ({ width, docked, overlay }: SidebarFacts) => (isNarrow(width) ? overlay : docked);

/** The saved fold choice is only touched while the window is wide; a narrow window opens and closes the overlay instead. */
export function applySidebar(facts: SidebarFacts, visible: boolean): { docked: boolean; overlay: boolean } {
  return isNarrow(facts.width) ? { docked: facts.docked, overlay: visible } : { docked: visible, overlay: false };
}

/** A rail pick opens the overlay only for modules that have no page of their own; page modules show their page. */
export const overlayOnReveal = (ownsPage: boolean) => !ownsPage;
