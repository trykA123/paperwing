<script lang="ts">
  import type { RailSection } from '../lib/api';
  import { RAIL_SECTIONS } from '../lib/rail';
  import { app } from '../lib/state.svelte';
  import BrandMark from './BrandMark.svelte';
  import Icon from './Icon.svelte';

  let { gitBusy }: { gitBusy: boolean } = $props();

  const comparisons = $derived(app.tabs.filter(tab => tab.view.kind === 'compare' || tab.view.kind === 'setCompare').length);
  const failed = $derived(app.activity.filter(entry => entry.state === 'failed' || entry.state === 'timedOut').length);
  const badges = $derived<Partial<Record<RailSection, { count: number; tone?: 'err' }>>>({
    compare: { count: comparisons },
    activity: failed ? { count: failed, tone: 'err' } : { count: app.activity.filter(entry => entry.state === 'running').length },
  });
  const KEYS = ['Ctrl+1', 'Ctrl+2', 'Ctrl+3', 'Ctrl+4'];
</script>

<nav class="activity-rail" aria-label="Sections">
  <span class="rail-mark"><BrandMark busy={gitBusy} size={28} /></span>
  {#each RAIL_SECTIONS as entry, index (entry.id)}
    {@const active = app.ws.shell.sidebarVisible && app.ws.shell.section === entry.id}
    {@const badge = badges[entry.id]}
    <button class="rail-btn" class:on={active} title="{entry.label} ({KEYS[index]})" aria-label={entry.label}
      aria-pressed={active} onclick={() => app.clickRail(entry.id)}>
      <Icon name={entry.icon} size={18} />
      {#if badge?.count}<span class="rail-badge" class:err={badge.tone === 'err'}><span class="sr-only">{badge.count} </span><span aria-hidden="true">{badge.count > 9 ? '9+' : badge.count}</span></span>{/if}
    </button>
  {/each}
  <span class="grow"></span>
  <button class="rail-btn" class:on={app.view.kind === 'settings'} title="Settings (Ctrl+5)" aria-label="Settings"
    aria-pressed={app.view.kind === 'settings'} onclick={() => app.openView({ kind: 'settings' })}>
    <Icon name="gear" size={18} />
  </button>
</nav>
