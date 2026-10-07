import type { RailSection } from './api';

export type ModuleCopy = { title: string; sub: string; note: string };

/** Words for modules whose page is not built yet. */
export const PLACEHOLDER_COPY: Partial<Record<RailSection, ModuleCopy>> = {
  changes: { title: 'Changes', sub: 'Uncommitted work across the repositories of a set', note: 'This page is not built yet. Commit from a repository row menu in the meantime.' },
  actions: { title: 'Workflow runs', sub: 'CI runs across the repositories of a set', note: 'This page is not built yet. Runs will list here with their jobs and logs.' },
  releases: { title: 'Releases', sub: 'GitHub releases across the repositories of a set', note: 'This page is not built yet.' },
  jira: { title: 'Jira issues', sub: 'Issues linked to the branches of a set', note: 'This page is not built yet. Issues will list here with their branch and pull request.' },
};
