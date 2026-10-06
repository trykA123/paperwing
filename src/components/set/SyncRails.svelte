<script lang="ts">
  import { RAIL_STEP, shownDots, type SyncView } from '../../lib/formation';
  import Icon from '../Icon.svelte';

  let { view, compact = false }: { view: SyncView; compact?: boolean } = $props();

  const BASE = 9, LOCAL_Y = 21, ORIGIN_Y = 7;
  const ahead = $derived(view.kind === 'rails' ? shownDots(view.ahead) : 0);
  const behind = $derived(view.kind === 'rails' ? shownDots(view.behind) : 0);
  const tip = $derived(BASE + Math.max(ahead, 1) * RAIL_STEP);
  const reach = $derived(Math.max(tip, behind ? BASE + behind * RAIL_STEP + 4 : 0));
  const ring = $derived(view.kind === 'rails' && view.dirty > 99 ? 13 : 10);
  const width = $derived(reach + (view.kind === 'rails' && view.dirty ? 2 * ring + 10 : 8));
</script>

{#if view.kind === 'rails'}
  <span class="fm-sync" class:compact title={view.label}>
    <span class="sr-only">{view.label}</span>
    {#if view.inSync}
      <span class="fm-insync" aria-hidden="true"><Icon name="check" tone="ok" />In sync</span>
    {:else if view.unpublished}
      <span class="fm-insync" aria-hidden="true"><Icon name="upload" />Not published</span>
    {:else}
      <svg class="fm-rails" width={width} height="32" viewBox="0 0 {width} 32" aria-hidden="true">
        <path class="rail" d="M{BASE} {LOCAL_Y}H{tip}" />
        {#if behind}<path class="rail origin" d="M{BASE} {LOCAL_Y}C{BASE + 6} {LOCAL_Y} {BASE + 4} {ORIGIN_Y} {BASE + RAIL_STEP} {ORIGIN_Y}H{BASE + behind * RAIL_STEP + 4}" />{/if}
        <circle class="base" cx={BASE} cy={LOCAL_Y} r="3.5" />
        {#each { length: behind } as _, k}<circle class="behind" cx={BASE + (k + 1) * RAIL_STEP} cy={ORIGIN_Y} r="4" />{/each}
        {#each { length: ahead } as _, k}<circle class="ahead" cx={BASE + (k + 1) * RAIL_STEP} cy={LOCAL_Y} r="4.5" />{/each}
        {#if view.dirty}
          <circle class="dirty" cx={reach + 6 + ring} cy={LOCAL_Y} r={ring} />
          <text class="dirty-n" x={reach + 6 + ring} y={LOCAL_Y} text-anchor="middle" dominant-baseline="central">{view.dirty > 99 ? '99+' : view.dirty}</text>
        {/if}
      </svg>
      <span class="fm-counts" aria-hidden="true">
        {#if view.ahead}<span class="up">↑{view.ahead}</span>{/if}
        {#if view.behind}<span class="down">↓{view.behind}</span>{/if}
      </span>
    {/if}
  </span>
{:else if view.kind === 'missing'}
  <span class="fm-quiet">Not cloned</span>
{:else if view.kind === 'broken'}
  <span class="fm-broken" title={view.reason}><Icon name="alert" tone="warn" />{view.reason}</span>
{:else if view.kind === 'unavailable'}
  <span class="fm-broken" title={view.reason}><Icon name="alert" tone="warn" />Status unavailable: {view.reason}</span>
{:else}
  <span class="fm-quiet">Checking…</span>
{/if}
