<script lang="ts">
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';

  const store = app.repositories;
  const note = $derived.by(() => {
    if (!app.ws.stars.length) return 'No favorites yet. Star a repository.';
    if (Object.values(app.loadingRepos).some(Boolean)) return 'Loading favorites…';
    return Object.values(app.repoErrors).some(errors => errors.length) ? 'Favorites unavailable' : 'No favorites';
  });

  function show(name: string) {
    store.filter({ chip: 'all', hostFilter: '', org: '', query: name });
    app.openView({ kind: 'repos' });
  }
</script>

<div class="sec">
  <h6>Favorites</h6>
  {#each store.favorites as entry (entry.key)}
    {@const local = store.localOf(entry)}
    <button class="nav fav" title="Show {entry.org}/{entry.name}" onclick={() => show(entry.name)}>
      <span class="star"><Icon name="star" size={12} /></span><span class="lbl">{entry.name} <small>{entry.org}</small></span>
      <span class="rf-mini">
        {#if local?.dirty}<span class="rf-cnt-dirty" title="{local.dirty} uncommitted files">{local.dirty}</span>{/if}{#if local?.ahead}<span class="rf-cnt-up" title="{local.ahead} ahead">↑{local.ahead}</span>{/if}{#if local?.behind}<span class="rf-cnt-down" title="{local.behind} behind">↓{local.behind}</span>{/if}
      </span>
    </button>
  {:else}<div class="nav ghost">{note}</div>{/each}
</div>
