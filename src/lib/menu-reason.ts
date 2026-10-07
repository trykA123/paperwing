export type Need = 'cloned' | 'managed' | 'behind' | 'unpushed' | 'offRef' | 'dirty' | 'branch';

export type MenuFacts = {
  ready: boolean; cloned: boolean; inPlace: boolean; idle: boolean; preparing: boolean;
  behind: number; ahead: number; dirty: number; onRef: boolean; hasBranch: boolean;
};

/** Why a menu item is disabled, from what is true of the row; the first failing fact wins. */
export function disabledReason(needs: readonly Need[], facts: MenuFacts): string | null {
  if (!facts.ready) return 'Skein is still loading';
  if (facts.preparing) return 'Preparing a clone';
  if (!facts.idle) return 'A Git operation is already running';
  if (needs.includes('managed') && facts.inPlace) return 'Not available for folders opened in place yet';
  if (needs.includes('cloned') && !facts.cloned) return 'Clone the repository first';
  if (needs.includes('behind') && facts.behind <= 0) return 'Not behind its upstream';
  if (needs.includes('unpushed') && facts.ahead <= 0) return 'Nothing to push';
  if (needs.includes('offRef') && facts.onRef) return 'Already on the set’s branch';
  if (needs.includes('dirty') && facts.dirty <= 0) return 'No uncommitted changes';
  if (needs.includes('branch') && !facts.hasBranch) return 'Not on a branch';
  return null;
}
