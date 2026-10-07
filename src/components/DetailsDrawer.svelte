<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { fade, fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { app } from '../lib/state.svelte';
  import { motionMs } from '../lib/appearance';
  import { detailsDrawer, type DrawerTarget } from '../lib/details-drawer.svelte';
  import { drawerTitle } from '../lib/drawer-title';
  import { containFocus, trapTab } from '../lib/focus-trap';
  import { hostOfItem } from '../lib/repositories';
  import CommitBody from './drawer/CommitBody.svelte';
  import HistoryBody from './drawer/HistoryBody.svelte';
  import PullBody from './drawer/PullBody.svelte';
  import QuickLook from './drawer/QuickLook.svelte';
  import StashBody from './drawer/StashBody.svelte';
  import Icon from './Icon.svelte';

  let { target }: { target: DrawerTarget } = $props();
  let panel: HTMLElement;
  let alive = true;
  const path = $derived(target.kind === 'repository' ? app.dest(target.item) : target.kind === 'pull' ? '' : target.path);
  const words = $derived(drawerTitle(target, {
    folder: target.kind === 'repository' ? app.folderOf(target.item) : '', host: target.kind === 'repository' ? hostOfItem(target.item, app.sources) : '',
    branch: path ? app.local[path]?.branch : null,
  }));

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
    const opener = detailsDrawer.opener;
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
    detailsDrawer.close();
  }
  const onKey = (event: KeyboardEvent) => trapTab(event, panel);
  const slide = () => ({ x: '100%', opacity: 1, duration: motionMs(220), easing: cubicOut });
</script>

<div class="history-scrim" role="presentation" onclick={() => detailsDrawer.close()} transition:fade|global={{ duration: motionMs(180) }}></div>
<div class="history-drawer" role="dialog" aria-modal="true" aria-label={words.label} tabindex="-1" bind:this={panel} transition:fly|global={slide()} onkeydown={onKey}>
  <header class="history-head">
    <div class="history-title">
      <h2 title={words.title}>{words.title}</h2>
      {#if words.sub}<span class="history-branch mono">{#if target.kind === 'history'}<Icon name="branch" tone="branch" />{/if}{words.sub}</span>{/if}
    </div>
    {#if target.kind === 'history'}<button class="btn small" title="Delete branches that are already merged" onclick={() => { app.cleanupDialog ??= { targets: [{ path: target.path, name: target.name }] }; }}><Icon name="trash" />Clean up branches</button>{/if}
    <button class="shell-control" title="Close" aria-label="Close" onclick={() => detailsDrawer.close()}><Icon name="close" /></button>
  </header>
  {#if target.kind === 'repository'}<QuickLook item={target.item} onready={settled} />
  {:else if target.kind === 'history'}<HistoryBody {target} onready={settled} />
  {:else if target.kind === 'commit'}<CommitBody {target} />
  {:else if target.kind === 'pull'}<PullBody {target} />
  {:else}<StashBody {target} onready={settled} />{/if}
</div>
