<script lang="ts">
  import { awaitingByHost } from '../lib/host-counts';
  import { railLayout, shortcutLabel, type ModuleDef, type ModuleId, type ProviderEntry, type RailBadge } from '../lib/modules';
  import { RailFlyoutState } from '../lib/rail-flyout.svelte';
  import { pulls } from '../lib/pulls.svelte';
  import { app } from '../lib/state.svelte';
  import BrandMark from './BrandMark.svelte';
  import Icon from './Icon.svelte';
  import RailFlyout from './RailFlyout.svelte';

  let { gitBusy }: { gitBusy: boolean } = $props();

  const flyout = new RailFlyoutState();
  const comparisons = $derived(app.tabs.filter(tab => tab.view.kind === 'compare' || tab.view.kind === 'setCompare').length);
  const layout = $derived(railLayout(app.sources, {
    comparisons, awaitingReview: pulls.awaitingReview, failedRuns: 0,
    gitFailed: app.activity.filter(entry => entry.state === 'failed' || entry.state === 'timedOut').length,
    gitRunning: app.activity.filter(entry => entry.state === 'running').length,
  }));
  const shown = $derived(layout.providers.find(entry => entry.provider.id === flyout.open?.id));
  const isOn = (module: ModuleDef) => module.id === 'settings' ? app.view.kind === 'settings' : app.ws.shell.sidebarVisible && app.ws.shell.section === module.id;
  const titleOf = (module: ModuleDef) => { const keys = shortcutLabel(module); return keys ? `${module.label} (${keys})` : module.label; };
  const buttonOf = (id: string) => document.querySelector<HTMLElement>(`.rail-btn[data-provider="${id}"]`);

  function countFor(host: string, id: ModuleId): RailBadge | undefined {
    if (id !== 'prs' || !shown) return undefined;
    const repoIds = new Map(app.ws.sets.flatMap(set => set.items.map(item => [app.dest(item, set.id), item.repoId] as const)));
    const count = awaitingByHost(pulls.awaitingPaths, repoIds, shown.hosts)[host];
    return count ? { count, tone: 'acc' } : undefined;
  }

  function close(restoreFocus: boolean) {
    const id = flyout.open?.id;
    flyout.close();
    if (restoreFocus && id) buttonOf(id)?.focus();
  }

  function pick(id: ModuleId) {
    app.showModule(id);
    close(true);
  }

  function triggerKey(event: KeyboardEvent, entry: ProviderEntry) {
    if (event.key === 'ArrowRight' || event.key === 'ArrowDown') { event.preventDefault(); flyout.show(entry.provider.id, true); }
    else if (event.key === 'Escape' && flyout.open) { event.preventDefault(); close(false); }
  }

  $effect(() => {
    const open = flyout.open;
    if (!open) return;
    const outside = (event: PointerEvent) => {
      const target = event.target as Element;
      if (open.mode === 'pinned' && !target.closest('.rail-flyout, .rail-btn[data-provider]')) flyout.close();
    };
    const move = (event: PointerEvent) => {
      const panel = document.querySelector('.rail-flyout');
      if (open.mode === 'hover' && panel) flyout.move({ x: event.clientX, y: event.clientY }, panel.getBoundingClientRect());
    };
    const escape = (event: KeyboardEvent) => { if (event.key === 'Escape') close(false); };
    document.addEventListener('pointerdown', outside, true);
    document.addEventListener('pointermove', move);
    document.addEventListener('keydown', escape);
    return () => {
      document.removeEventListener('pointerdown', outside, true);
      document.removeEventListener('pointermove', move);
      document.removeEventListener('keydown', escape);
    };
  });
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
    {@const id = entry.provider.id}
    <button class="rail-btn" class:on={entry.items.some(item => isOn(item.module))} class:open={flyout.open?.id === id} data-provider={id} aria-haspopup="menu" aria-expanded={flyout.open?.id === id}
      aria-label="{entry.provider.label}, {entry.hosts.map(host => host.host).join(', ')}" title="{entry.provider.label} · {entry.hosts.map(host => host.host).join(', ')}" data-tip-side="right"
      onpointerenter={event => event.pointerType === 'mouse' && flyout.enterTrigger(id)} onpointerleave={event => event.pointerType === 'mouse' && flyout.leave({ x: event.clientX, y: event.clientY })}
      onclick={event => flyout.toggle(id, event.detail === 0)} onkeydown={event => triggerKey(event, entry)}>
      <Icon name={entry.provider.icon} size={18} />{@render badgeOf(entry.badge)}
    </button>
  {/each}
  <span class="grow"></span>
  {#each layout.system as item (item.module.id)}{@render moduleButton(item.module, item.badge)}{/each}
</nav>

{#if shown && flyout.open}
  {@const anchor = buttonOf(shown.provider.id)}
  {#if anchor}
    {#key flyout.open.id}
      <RailFlyout entry={shown} {anchor} focusFirst={flyout.open.focus} current={app.ws.shell.sidebarVisible ? app.ws.shell.section : null} count={countFor}
        onpick={pick} onclose={close} onenter={() => flyout.enterPanel()} onleave={event => flyout.leave({ x: event.clientX, y: event.clientY })} />
    {/key}
  {/if}
{/if}
