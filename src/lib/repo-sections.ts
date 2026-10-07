import type { IconName } from '../components/Icon.svelte';

export type RepoSection = 'overview' | 'changes' | 'history' | 'branches' | 'stash' | 'prs' | 'actions' | 'compare';
export type RepoSectionDef = { id: RepoSection; label: string; icon: IconName; needsClone: boolean; title: string };

export const REPO_SECTIONS: readonly RepoSectionDef[] = [
  { id: 'overview', label: 'Overview', icon: 'repo', needsClone: false, title: 'Overview' },
  { id: 'changes', label: 'Changes', icon: 'changes', needsClone: true, title: 'Changes' },
  { id: 'history', label: 'History', icon: 'commit', needsClone: true, title: 'History' },
  { id: 'branches', label: 'Branches & tags', icon: 'branch', needsClone: false, title: 'Branches and tags' },
  { id: 'stash', label: 'Stash', icon: 'stash', needsClone: true, title: 'Stash' },
  { id: 'prs', label: 'Pull requests', icon: 'pr', needsClone: true, title: 'Pull requests' },
  { id: 'actions', label: 'Actions', icon: 'actions', needsClone: false, title: 'Workflow runs' },
  { id: 'compare', label: 'Compare', icon: 'copy', needsClone: true, title: 'Compare' },
];

export const isRepoSection = (value: unknown): value is RepoSection => REPO_SECTIONS.some(section => section.id === value);
export const sectionById = (id: RepoSection): RepoSectionDef => REPO_SECTIONS.find(section => section.id === id)!;

/** A section that needs the clone falls back to Overview while the repository is only on the remote. */
export const usableSection = (id: RepoSection, cloned: boolean): RepoSection => (!cloned && sectionById(id).needsClone ? 'overview' : id);

export type RepoCounts = { changes: number; branches: number | null; stash: number | null; prs: number | null };

/** The number beside a section in the sidebar; null when it has none or it is not known yet. */
export function sectionCount(id: RepoSection, counts: RepoCounts): number | null {
  switch (id) {
    case 'changes': return counts.changes;
    case 'branches': return counts.branches;
    case 'stash': return counts.stash;
    case 'prs': return counts.prs;
    default: return null;
  }
}
