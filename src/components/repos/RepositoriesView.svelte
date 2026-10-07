<script lang="ts">
  import { bulkTargets } from '../../lib/formation';
  import { plural } from '../../lib/plural';
  import { rowFacts } from '../../lib/row-actions';
  import { app } from '../../lib/state.svelte';
  import EmptyState from '../EmptyState.svelte';
  import Icon from '../Icon.svelte';
  import PageFrame from '../module/PageFrame.svelte';
  import SetHeader from '../set/SetHeader.svelte';
  import RepoFilterBar from './RepoFilterBar.svelte';
  import RepositoryTable from './RepositoryTable.svelte';

  const store = app.repositories;
  const everything = $derived(store.everything);
  const cloned = $derived(everything.filter(entry => !entry.remoteOnly && store.localOf(entry)?.repo).length);
  const hosts = $derived(store.tree.length);
  const fetchable = $derived(bulkTargets(everything.filter(entry => !entry.remoteOnly).map(entry => entry.item), rowFacts).fetchable);
  const busy = $derived(app.running || app.gitBusy || app.clonePreparing);
  const sub = $derived(store.inSetView
    ? `${store.shown.length} of ${plural(store.entries.length, 'repository', 'repositories')} shown`
    : `${store.shown.length.toLocaleString('en')} shown · ${everything.length.toLocaleString('en')} known · ${cloned} cloned · ${everything.filter(entry => entry.remoteOnly).length.toLocaleString('en')} remote only on ${plural(hosts, 'host')}`);
  const items = $derived(store.shown.map(entry => entry.item));

  $effect(() => { if (!store.inSetView) store.refreshStatus(); });
</script>

<PageFrame crumb="Local Git" title="Repositories" {sub}>
  {#snippet buttons()}
    {#if !store.inSetView}
      <button class="btn" disabled={busy || !fetchable.length} title="git fetch --prune in every cloned repository" onclick={() => app.startClone(fetchable, 'fetch')}><Icon name="refresh" /> Fetch cloned</button>
    {/if}
  {/snippet}
  {#snippet chips()}<RepoFilterBar />{/snippet}
  {#if store.inSetView}<SetHeader />{/if}
  <RepositoryTable {items} label={store.inSetView ? `Repositories in ${app.set.name}` : 'Repositories'} scopeKey={store.inSetView ? app.set.id : 'all'}>
    {#snippet none()}
      {#if store.entries.length}
        <EmptyState icon="folder" title="Nothing matches" hint="No repository fits these filters right now.">
          <button class="btn" onclick={() => store.clearFilters()}>Clear filters</button>
        </EmptyState>
      {:else if app.isTemporary}
        <EmptyState icon="folder" title={app.temporary.find(app.set.id)?.scanning ? 'Scanning for repositories' : 'No repositories found'} hint="Repositories appear here as the scan finds them." />
      {:else if store.inSetView}
        <EmptyState icon="folder" title="This set is empty" hint="Select repositories in the list of all repositories and add them to this set.">
          <button class="btn dark" onclick={() => app.openView({ kind: 'repos' })}><Icon name="repo" /> Show all repositories</button>
        </EmptyState>
      {:else}
        <EmptyState icon="folder" title="No repositories yet" hint="Add a source in Settings, or open a folder with Skein.">
          <button class="btn dark" onclick={() => app.openView({ kind: 'settings' })}><Icon name="gear" /> Open Settings</button>
        </EmptyState>
      {/if}
    {/snippet}
  </RepositoryTable>
</PageFrame>
