import type { LocalStatus, SetItem } from './api';
import { stashable, switchable } from './stash-flow.svelte';
import { app } from './state.svelte';
import { taggable } from './tag-flow.svelte';

export type BranchSection = 'cleanup' | 'tags' | 'stash';
export const BRANCH_SECTIONS: readonly { id: BranchSection; label: string; title: string; hint: string }[] = [
  { id: 'cleanup', label: 'Clean up branches', title: 'Clean up merged branches', hint: 'Delete local and remote branches that are already merged.' },
  { id: 'tags', label: 'Tags', title: 'Tags', hint: 'Create tags, or delete a tag locally and on the remote.' },
  { id: 'stash', label: 'Stash', title: 'Stash', hint: 'Set uncommitted changes aside, or switch branch with a stash.' },
];

/** What the folder has checked out: its branch, else its tag, else a short commit. */
export const checkedOut = (local: LocalStatus | undefined) => local?.branchLabel ?? local?.branch ?? local?.tagLabel ?? local?.tag ?? local?.sha?.slice(0, 8) ?? '';

/** The cloned repositories of the active set, grouped by the flow each section serves. */
class BranchesModule {
  section = $state<BranchSection>('cleanup');
  page = $state(0);

  cloned = $derived(taggable(app.set.items));
  dirty = $derived(stashable(app.set.items));
  rows = $derived<SetItem[]>(this.section === 'stash' ? this.dirty : this.cloned);
  canSwitch = (item: SetItem) => switchable([item]).length > 0;

  select(section: BranchSection) { this.section = section; this.page = 0; }
}

export const branches = new BranchesModule();
