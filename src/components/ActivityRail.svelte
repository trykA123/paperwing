<script lang="ts">
  import { railLayout, moduleById, shortcutLabel, type ModuleDef, type RailBadge } from '../lib/modules';
  import { pulls } from '../lib/pulls.svelte';
  import { app } from '../lib/state.svelte';
  import BrandMark from './BrandMark.svelte';
  import Icon from './Icon.svelte';

  let { gitBusy }: { gitBusy: boolean } = $props();

  const comparisons = $derived(app.tabs.filter(tab => tab.view.kind === 'compare' || tab.view.kind === 'setCompare').length);
  const layout = $derived(railLayout(app.sources, {
    comparisons, awaitingReview: pulls.awaitingReview, failedRuns: 0,
    gitFailed: app.activity.filter(entry => entry.state === 'failed' || entry.state === 'timedOut').length,
    gitRunning: app.activity.filter(entry => entry.state === 'running').length,
  }));
  const isOn = (module: ModuleDef) => module.id === 'settings' ? app.view.kind === 'settings' : app.ws.shell.sidebarVisible && app.ws.shell.section === module.id;
  const titleOf = (module: ModuleDef) => { const keys = shortcutLabel(module); return keys ? `${module.label} (${keys})` : module.label; };
</script>

{#snippet badgeOf(badge: RailBadge | undefined)}
  {#if badge}<span class="rail-badge" class:err={badge.tone === 'err'}><span class="sr-only">{badge.count} </span><span aria-hidden="true">{badge.count > 99 ? '99+' : badge.count}</span></span>{/if}
{/snippet}

{#snippet moduleButton(module: ModuleDef, badge: RailBadge | undefined)}
  {@const on = isOn(module)}
  <button class="rail-btn" class:on title={titleOf(module)} aria-label={module.label} data-tip-side="right" aria-pressed={on} onclick={() => app.openModule(module.id)}>
    <Icon name={module.icon} size={18} />{@render badgeOf(badge)}
  </button>
{/snippet}

<nav class="activity-rail" aria-label="Modules">
  <span class="rail-mark" data-tauri-drag-region={app.platform.platform === 'windows' ? true : undefined}><BrandMark busy={gitBusy} size={28} /></span>
  {#each layout.local as item (item.module.id)}{@render moduleButton(item.module, item.badge)}{/each}
  {#if layout.providers.length}<span class="rail-sep" aria-hidden="true"></span>{/if}
  {#each layout.providers as entry (entry.provider.id)}
    {@const first = entry.items[0].module}
    {@const on = entry.items.some(item => isOn(item.module))}
    <button class="rail-btn" class:on title="{entry.provider.label} · {entry.hosts.map(host => host.host).join(', ')}" aria-label={entry.provider.label} data-tip-side="right"
      aria-pressed={on} onclick={() => app.openModule(first.id)}>
      <Icon name={entry.provider.icon} size={18} />{@render badgeOf(entry.badge)}
    </button>
  {/each}
  <span class="grow"></span>
  {#each layout.system as item (item.module.id)}{@render moduleButton(item.module, item.badge)}{/each}
</nav>
