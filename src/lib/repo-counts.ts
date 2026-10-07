import type { RepoEntry } from './repositories';
import type { RepoCounts } from './repo-sections';
import { pullKey } from './pull-flow.svelte';
import { pulls } from './pulls.svelte';
import { app } from './state.svelte';

/** What each section of a repository page counts, read from what the app already loaded; null while it is not known. */
export function repoCounts(entry: RepoEntry): RepoCounts {
  const path = app.dest(entry.item);
  const tree = app.trees[path]?.data;
  const key = pullKey(entry.item);
  const answer = key ? pulls.entry(key) : undefined;
  return {
    changes: app.local[path]?.dirty ?? 0,
    branches: tree ? tree.branches.length + tree.tags.length : null,
    stash: tree ? tree.stashes.length : null,
    prs: answer?.status === 'ready' ? (answer.pull && (answer.pull.state === 'open' || answer.pull.state === 'draft') ? 1 : 0) : null,
  };
}
