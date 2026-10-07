<script lang="ts">
  import { untrack } from 'svelte';
  import { isCloned } from '../../lib/formation';
  import { sectionById, usableSection, type RepoSection } from '../../lib/repo-sections';
  import { app } from '../../lib/state.svelte';
  import type { View } from '../../lib/workspace';
  import EmptyState from '../EmptyState.svelte';
  import Skeleton from '../Skeleton.svelte';
  import RepositoryHeader from './RepositoryHeader.svelte';
  import RepoActions from './section/RepoActions.svelte';
  import RepoBranches from './section/RepoBranches.svelte';
  import RepoChanges from './section/RepoChanges.svelte';
  import RepoCompare from './section/RepoCompare.svelte';
  import RepoHistory from './section/RepoHistory.svelte';
  import RepoOverview from './section/RepoOverview.svelte';
  import RepoPulls from './section/RepoPulls.svelte';
  import RepoStash from './section/RepoStash.svelte';

  let { view }: { view: Extract<View, { kind: 'repo' }> } = $props();

  const store = app.repositories;
  const entry = $derived(store.resolve(view.repoId));
  const path = $derived(entry ? app.dest(entry.item) : '');
  const cloned = $derived(!!entry && isCloned(app.local[path]));
  const section = $derived(usableSection(view.section, cloned));
  const loading = $derived(Object.values(app.loadingRepos).some(Boolean) || !app.ready);
  const goto = (next: RepoSection) => store.openRepository(view.repoId, next);

  $effect(() => {
    if (!entry || cloned || app.local[path]) return;
    const target = path;
    untrack(() => void app.checkExists([target]));
  });

  $effect(() => {
    if (!cloned) return;
    const target = path;
    const reading = new AbortController();
    untrack(() => { app.openTreePaths = [target]; void app.loadTree(target, false, reading.signal); });
    return () => { reading.abort(); app.openTreePaths = []; };
  });
</script>

{#if !entry}
  {#if loading}<Skeleton rows={6} height={40} />
  {:else}
    <EmptyState icon="folder" title="Repository not found" hint="It is no longer in a source or a set.">
      <button class="btn" onclick={() => store.back()}>Back to the list</button>
    </EmptyState>
  {/if}
{:else}
  <RepositoryHeader {entry} />
  <div class="rf-page" role="region" aria-label={sectionById(section).title}>
    {#if section === 'overview'}<RepoOverview {entry} {goto} />
    {:else if section === 'changes'}<RepoChanges {entry} />
    {:else if section === 'history'}<RepoHistory {entry} />
    {:else if section === 'branches'}<RepoBranches {entry} />
    {:else if section === 'stash'}<RepoStash {entry} />
    {:else if section === 'prs'}<RepoPulls {entry} />
    {:else if section === 'actions'}<RepoActions {entry} />
    {:else}<RepoCompare {entry} />{/if}
  </div>
{/if}
