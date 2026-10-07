import type { Component } from 'svelte';
import type { RailSection } from '../../lib/api';
import ReposSidebar from '../repos/ReposSidebar.svelte';
import ActivityPanel from '../panel/ActivityPanel.svelte';
import BranchesPanel from '../panel/BranchesPanel.svelte';
import ComparePanel from '../panel/ComparePanel.svelte';
import PlaceholderPanel from '../panel/PlaceholderPanel.svelte';
import PullsPanel from '../panel/PullsPanel.svelte';
import RecoveryEntry from '../panel/RecoveryEntry.svelte';
import SearchPanel from '../panel/SearchPanel.svelte';
import BranchesPage from './BranchesPage.svelte';
import PlaceholderPage from './PlaceholderPage.svelte';
import PullsPage from './PullsPage.svelte';

type ModuleView = Component;

/** The sidebar each module shows next to its page. */
export const SIDEBARS: Record<RailSection, ModuleView> = {
  repos: ReposSidebar, changes: PlaceholderPanel, branches: BranchesPanel, compare: ComparePanel, search: SearchPanel,
  prs: PullsPanel, actions: PlaceholderPanel, releases: PlaceholderPanel, jira: PlaceholderPanel, activity: ActivityPanel, recovery: RecoveryEntry,
};

/** The page a module opens as a tab; Sets, Search and Compare keep their own views. */
export const PAGES: Partial<Record<RailSection, ModuleView>> = {
  changes: PlaceholderPage, branches: BranchesPage, prs: PullsPage, actions: PlaceholderPage, releases: PlaceholderPage, jira: PlaceholderPage,
};
