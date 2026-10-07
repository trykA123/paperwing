<script lang="ts" generics="Id extends string">
  import Icon from '../Icon.svelte';

  let { filters, counts, value, onchange }: {
    filters: readonly { id: Id; label: string }[]; counts: Record<Id, number>; value: Id; onchange: (next: Id) => void;
  } = $props();

  function move(event: KeyboardEvent, index: number) {
    const step = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0;
    if (!step) return;
    event.preventDefault();
    const buttons = event.currentTarget instanceof HTMLElement ? event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>('button.fm-chip') : undefined;
    buttons?.[(index + step + filters.length) % filters.length]?.focus();
  }
</script>

{#each filters as filter, index (filter.id)}
  <button class="fm-chip {filter.id}" class:on={value === filter.id} aria-pressed={value === filter.id}
    onclick={() => onchange(filter.id)} onkeydown={event => move(event, index)}>
    {#if filter.id === 'favorites'}<Icon name="star" size={12} />{:else if filter.id !== 'all'}<i class="fm-dot {filter.id}" aria-hidden="true"></i>{/if}{filter.label} <b>{counts[filter.id]}</b>
  </button>
{/each}
