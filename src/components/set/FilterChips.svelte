<script lang="ts">
  import { FILTERS, type FilterCounts, type FormationFilter } from '../../lib/formation';

  let { counts, value, onchange }: { counts: FilterCounts; value: FormationFilter; onchange: (next: FormationFilter) => void } = $props();

  function move(event: KeyboardEvent, index: number) {
    const step = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0;
    if (!step) return;
    event.preventDefault();
    const buttons = event.currentTarget instanceof HTMLElement ? event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>('button') : undefined;
    buttons?.[(index + step + FILTERS.length) % FILTERS.length]?.focus();
  }
</script>

<div class="fm-filters" role="group" aria-label="Filter repositories">
  {#each FILTERS as filter, index (filter.id)}
    <button class="fm-chip {filter.id}" class:on={value === filter.id} aria-pressed={value === filter.id}
      onclick={() => onchange(filter.id)} onkeydown={event => move(event, index)}>
      {#if filter.id !== 'all'}<i class="fm-dot {filter.id}" aria-hidden="true"></i>{/if}{filter.label} <b>{counts[filter.id]}</b>
    </button>
  {/each}
</div>
