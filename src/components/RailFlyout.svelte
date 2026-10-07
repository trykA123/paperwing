<script lang="ts">
  import { menuStep, typeaheadMatch, TYPEAHEAD_RESET } from '../lib/flyout-hover';
  import type { ModuleId, ProviderEntry, RailBadge } from '../lib/modules';
  import Icon from './Icon.svelte';

  let { entry, anchor, focusFirst, current, count, onpick, onclose, onenter, onleave }: {
    entry: ProviderEntry; anchor: HTMLElement; focusFirst: boolean; current: ModuleId | null;
    count: (host: string, id: ModuleId) => RailBadge | undefined;
    onpick: (id: ModuleId) => void; onclose: (restoreFocus: boolean) => void; onenter: () => void; onleave: (event: PointerEvent) => void;
  } = $props();

  const EDGE = 8;
  let panel: HTMLDivElement;
  let top = $state(0);
  let left = $state(0);
  let active = $state(0);
  let typed = '';
  let typedAt: ReturnType<typeof setTimeout> | undefined;
  const rows = $derived(entry.hosts.flatMap(host => entry.items.map(item => ({ host: host.host, item: item.module }))));
  const items = () => [...panel.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')];

  $effect(() => {
    const box = anchor.getBoundingClientRect();
    top = Math.max(EDGE, Math.min(box.top - 6, innerHeight - panel.offsetHeight - EDGE));
    left = (anchor.closest('.activity-rail')?.getBoundingClientRect().right ?? box.right) + 4;
    if (focusFirst) items()[0]?.focus();
    return () => clearTimeout(typedAt);
  });

  function focusAt(index: number) {
    active = index;
    items()[index]?.focus();
  }

  function search(key: string) {
    clearTimeout(typedAt);
    typed += key;
    typedAt = setTimeout(() => { typed = ''; }, TYPEAHEAD_RESET);
    const at = typeaheadMatch(rows.map(row => row.item.itemLabel ?? row.item.label), active, typed);
    if (at >= 0) focusAt(at);
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'Escape' || event.key === 'ArrowLeft') { event.preventDefault(); onclose(true); return; }
    if (event.key === 'Tab') { onclose(true); return; }
    const next = menuStep(event.key, active, rows.length);
    if (next !== undefined) { event.preventDefault(); focusAt(next); return; }
    if (event.key.length === 1 && event.key !== ' ' && !event.ctrlKey && !event.altKey && !event.metaKey) { event.preventDefault(); search(event.key); }
  }
</script>

<div class="rail-flyout" role="menu" tabindex="-1" aria-label={entry.provider.label} bind:this={panel} style:top="{top}px" style:left="{left}px"
  onpointerenter={event => event.pointerType === 'mouse' && onenter()} onpointerleave={event => event.pointerType === 'mouse' && onleave(event)} {onkeydown}>
  <div class="fly-head">{entry.provider.label}</div>
  {#each entry.hosts as host, hostIndex (host.host)}
    <div class="fly-sec" role="group" aria-label={host.host}>
      <small class="fly-host">{host.host}</small>
      {#each entry.items as { module }, itemIndex (module.id)}
        {@const index = hostIndex * entry.items.length + itemIndex}
        {@const badge = count(host.host, module.id)}
        <button class="fly-item" class:on={module.id === current} role="menuitem" tabindex={index === active ? 0 : -1} onclick={() => onpick(module.id)} onfocus={() => (active = index)}>
          <span class="fly-ico"><Icon name={module.icon} /></span><span class="lbl">{module.itemLabel ?? module.label}</span>
          {#if badge}<span class="cnt" class:err={badge.tone === 'err'}>{badge.count}</span>{/if}
        </button>
      {/each}
    </div>
  {/each}
</div>
