<script lang="ts" generics="T">
  import type { Snippet } from 'svelte';
  import { app } from '../../lib/state.svelte';
  import Pager from '../Pager.svelte';
  import VirtualList from '../VirtualList.svelte';

  let { label, columns, cols, items, key, cells, empty, page = $bindable(0) }: {
    label: string; columns: readonly string[]; cols: string; items: T[]; key: (item: T) => string;
    cells: Snippet<[T, number]>; empty?: Snippet; page?: number;
  } = $props();

  const size = $derived(app.ws.pageSize);
  const density = $derived(app.ws.density ?? 'comfortable');
  const pages = $derived(size === 'all' ? 1 : Math.max(1, Math.ceil(items.length / size)));
  const current = $derived(Math.min(page, pages - 1));
  const rows = $derived(size === 'all' ? items : items.slice(current * size, current * size + size));
</script>

<div class="fm-wrap module-table" style:--mod-cols={cols}>
  <div class="card fill repository-table fm-table" class:compact={density === 'compact'}>
    {#key `${current}|${size}`}
      <VirtualList role="grid" {label} items={rows} rowHeight={density === 'compact' ? 40 : 56} {key} {empty}>
        {#snippet header()}
          <div class="fm-row fm-head" role="row">
            {#each columns as column (column)}<div class="fm-cell" role="columnheader">{column}</div>{/each}
          </div>
        {/snippet}
        {#snippet row(item: T, index: number)}{@render cells(item, index)}{/snippet}
      </VirtualList>
    {/key}
    {#if items.length}<Pager total={items.length} bind:page bind:size={app.ws.pageSize} {density} ondensity={value => (app.ws.density = value)} />{/if}
  </div>
</div>
