import type { RailSection } from './api';
import type { IconName } from '../components/Icon.svelte';

export type RailEntry = { id: RailSection; label: string; icon: IconName };

export const RAIL_SECTIONS: readonly RailEntry[] = [
  { id: 'sets', label: 'Sets', icon: 'layers' },
  { id: 'compare', label: 'Compare', icon: 'copy' },
  { id: 'recovery', label: 'Recovery', icon: 'undo' },
  { id: 'activity', label: 'Activity', icon: 'activity' },
];

export type RailState = { section: RailSection; sidebarVisible: boolean };

/** Clicking the active icon folds the panel; any other click shows that section. */
export function railClick(state: RailState, clicked: RailSection): RailState {
  if (state.sidebarVisible && state.section === clicked) return { section: clicked, sidebarVisible: false };
  return { section: clicked, sidebarVisible: true };
}

/** Ctrl+1..4 pick a section, Ctrl+5 opens Settings. */
export function railShortcut(key: string): RailSection | 'settings' | undefined {
  const index = Number(key) - 1;
  if (!Number.isInteger(index) || index < 0) return undefined;
  return index < RAIL_SECTIONS.length ? RAIL_SECTIONS[index].id : index === RAIL_SECTIONS.length ? 'settings' : undefined;
}
