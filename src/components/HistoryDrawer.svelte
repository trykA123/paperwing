<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { fade, fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { app } from '../lib/state.svelte';
  import { motionMs } from '../lib/appearance';
  import { containFocus, trapTab } from '../lib/focus-trap';
  import type { GraphRow } from '../lib/history-graph';
  import { historyDrawer, type HistoryTarget } from '../lib/history-drawer.svelte';
  import HistoryDetails from './HistoryDetails.svelte';
  import HistoryPanel from './HistoryPanel.svelte';
  import Icon from './Icon.svelte';
  import StashSection from './stash/StashSection.svelte';
  import TagSection from './tags/TagSection.svelte';

  let { target }: { target: HistoryTarget } = $props();
  let panel: HTMLElement;
  let active = $state<GraphRow | null>(null);
  let alive = true;
  const branch = $derived(app.local[target.path]?.branch);

  function focusPanel(tries = 10) {
    if (!alive || panel.contains(document.activeElement)) return;
    panel.focus();
    if (document.activeElement !== panel && tries > 0) requestAnimationFrame(() => focusPanel(tries - 1));
  }

  async function settled() {
    await tick();
    focusPanel();
  }

  onMount(() => {
    const opener = historyDrawer.opener;
    focusPanel();
    const release = containFocus(panel);
    document.addEventListener('keydown', onEscape);
    return () => {
      alive = false; release(); document.removeEventListener('keydown', onEscape);
      if (opener?.isConnected) opener.focus();
    };
  });

  function onEscape(event: KeyboardEvent) {
    if (event.key !== 'Escape' || event.defaultPrevented || document.querySelector('dialog[open]')) return;
    event.preventDefault();
    historyDrawer.close();
  }
  const onKey = (event: KeyboardEvent) => trapTab(event, panel);
  const slide = () => ({ x: '100%', opacity: 1, duration: motionMs(220), easing: cubicOut });
</script>

<div class="history-scrim" role="presentation" onclick={() => historyDrawer.close()} transition:fade|global={{ duration: motionMs(180) }}></div>
<div class="history-drawer" role="dialog" aria-modal="true" aria-label="History of {target.name}" tabindex="-1" bind:this={panel}
  transition:fly|global={slide()} onkeydown={onKey}>
  <header class="history-head">
    <div class="history-title">
      <h2>{target.name}</h2>
      {#if branch !== undefined}<span class="history-branch mono"><Icon name="branch" tone="branch" />{branch ?? 'detached HEAD'}</span>{/if}
    </div>
    <button class="btn small" title="Delete branches that are already merged" onclick={() => { app.cleanupDialog ??= { targets: [{ path: target.path, name: target.name }] }; }}><Icon name="trash" />Clean up branches</button>
    <button class="shell-control" title="Close history" aria-label="Close history" onclick={() => historyDrawer.close()}><Icon name="close" /></button>
  </header>
  <div class="history-body">
    <TagSection path={target.path} name={target.name} />
    <StashSection path={target.path} name={target.name} />
    <HistoryPanel path={target.path} bind:active onloaded={settled} />
  </div>
  <HistoryDetails row={active} />
</div>
