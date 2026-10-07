<script lang="ts">
  import { REPO_FILTERS } from '../../lib/repositories';
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';
  import FilterChips from '../set/FilterChips.svelte';
  import SetChipMenu from './SetChipMenu.svelte';

  const store = app.repositories;
  let setChip = $state<HTMLButtonElement>();
  let menuOpen = $state(false);
  const scoped = $derived(store.inSetView ? app.set.name : null);
</script>

<div class="rf-filters">
  <div class="fm-filters" role="group" aria-label="Filter repositories">
    <FilterChips filters={REPO_FILTERS} counts={store.counts} value={store.chip} partial={store.partial ? ['cloned', 'changes', 'behind'] : []} onchange={chip => store.filter({ chip })} />
    <button class="fm-chip rf-setchip" class:on={!!scoped} bind:this={setChip} aria-haspopup="dialog" aria-expanded={menuOpen} onclick={() => (menuOpen = !menuOpen)}>
      <Icon name="folder" size={14} tone={scoped ? undefined : 'folder'} />{scoped ?? 'Any set'}<span class="car" aria-hidden="true">▾</span>
    </button>
    {#if store.hostFilter}<button class="fm-chip on" title="Remove the host filter" onclick={() => store.filter({ hostFilter: '', org: '' })}>{store.hostFilter} <span aria-hidden="true">✕</span><span class="sr-only">Remove host filter</span></button>{/if}
    {#if store.org}<button class="fm-chip on" title="Remove the organization filter" onclick={() => store.filter({ org: '' })}>{store.org} <span aria-hidden="true">✕</span><span class="sr-only">Remove organization filter</span></button>{/if}
  </div>
  <label class="gsearch small rf-search"><Icon name="search" /><input placeholder="Filter by name…" aria-label="Filter repositories by name" value={store.query} oninput={event => store.filter({ query: event.currentTarget.value })} spellcheck="false" /></label>
</div>

{#if menuOpen && setChip}<SetChipMenu anchor={setChip} onclose={() => (menuOpen = false)} />{/if}
