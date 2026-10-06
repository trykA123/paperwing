<script lang="ts">
  import type { PageSize, RowDensity } from '../lib/api';

  let { total, page = $bindable(), size = $bindable(), density, ondensity }: { total: number; page: number; size: PageSize; density?: RowDensity; ondensity?: (value: RowDensity) => void } = $props();

  const SIZES: PageSize[] = [10, 25, 50, 'all'];
  const pages = $derived(size === 'all' ? 1 : Math.max(1, Math.ceil(total / size)));
  const cur = $derived(Math.min(page, pages - 1));
  const from = $derived(size === 'all' ? 0 : cur * size);
  const to = $derived(size === 'all' ? total : Math.min(total, from + size));
</script>

<div class="pager">
  <span class="mut">Show</span>
  <div class="seg">
    {#each SIZES as n}
      <button class:on={size === n} onclick={() => { size = n; page = 0; }}>{n === 'all' ? 'All' : n}</button>
    {/each}
  </div>
  {#if density && ondensity}
    <div class="seg" role="group" aria-label="Row density">
      <button class:on={density === 'comfortable'} aria-pressed={density === 'comfortable'} onclick={() => ondensity('comfortable')}>Comfortable</button>
      <button class:on={density === 'compact'} aria-pressed={density === 'compact'} onclick={() => ondensity('compact')}>Compact</button>
    </div>
  {/if}
  <span class="grow"></span>
  <span class="mut">{total ? `${from + 1}–${to} of ${total}` : '0 items'}</span>
  <button class="pgb" disabled={cur === 0} onclick={() => (page = cur - 1)} aria-label="Previous page">‹</button>
  <button class="pgb" disabled={cur >= pages - 1} onclick={() => (page = cur + 1)} aria-label="Next page">›</button>
</div>
