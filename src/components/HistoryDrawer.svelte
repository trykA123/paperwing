<script lang="ts">
  import { onMount } from 'svelte';
  import { fade, fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { api, type RepositoryHistory } from '../lib/api';
  import { motionMs } from '../lib/appearance';
  import { containFocus, trapTab } from '../lib/focus-trap';
  import { canLoadMore, describeHistory, hasSharedBase, layoutHistory } from '../lib/history-graph';
  import { historyDrawer, type HistoryTarget } from '../lib/history-drawer.svelte';
  import Alert from './Alert.svelte';
  import EmptyState from './EmptyState.svelte';
  import HistoryDetails from './HistoryDetails.svelte';
  import HistoryGraph from './HistoryGraph.svelte';
  import Icon from './Icon.svelte';
  import Skeleton from './Skeleton.svelte';

  const FIRST_PAGE = 50;
  const LAST_PAGE = 200;
  type Load = { status: 'loading' } | { status: 'ready'; history: RepositoryHistory } | { status: 'error'; message: string };

  let { target }: { target: HistoryTarget } = $props();
  let panel: HTMLElement;
  let load = $state<Load>({ status: 'loading' });
  let limit = $state(FIRST_PAGE);
  let activeId = $state<string | null>(null);
  let alive = true;

  const history = $derived(load.status === 'ready' ? load.history : null);
  const layout = $derived(history ? layoutHistory(history) : null);
  const defaultId = $derived((layout?.rows.find(row => row.commit) ?? layout?.rows[0])?.id ?? null);
  const shownId = $derived(activeId ?? defaultId);
  const active = $derived(layout?.rows.find(row => row.id === shownId) ?? null);

  async function read(next: number) {
    limit = next;
    try {
      const result = await api.repositoryHistory(target.path, next);
      if (alive) load = { status: 'ready', history: result };
    } catch (error) {
      if (alive) load = { status: 'error', message: String(error) };
    }
  }

  onMount(() => {
    const previous = document.activeElement as HTMLElement | null;
    panel.focus();
    void read(FIRST_PAGE);
    const release = containFocus(panel);
    return () => { alive = false; release(); if (previous?.isConnected) previous.focus(); };
  });

  function onKey(event: KeyboardEvent) {
    if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); historyDrawer.close(); return; }
    trapTab(event, panel);
  }
  const slide = () => ({ x: '100%', opacity: 1, duration: motionMs(220), easing: cubicOut });
</script>

<div class="history-scrim" role="presentation" onclick={() => historyDrawer.close()} transition:fade|global={{ duration: motionMs(180) }}></div>
<div class="history-drawer" role="dialog" aria-modal="true" aria-label="History of {target.name}" tabindex="-1" bind:this={panel}
  transition:fly|global={slide()} onkeydown={onKey}>
  <header class="history-head">
    <div class="history-title">
      <h2>{target.name}</h2>
      {#if history}<span class="history-branch mono"><Icon name="branch" tone="branch" />{history.branch ?? 'detached HEAD'}</span>{/if}
    </div>
    <button class="shell-control" title="Close history" aria-label="Close history" onclick={() => historyDrawer.close()}><Icon name="close" /></button>
  </header>
  <div class="history-body">
    {#if load.status === 'error'}
      <Alert kind="err" role="alert">{load.message}{#snippet action()}<button class="btn small" onclick={() => read(limit)}>Retry</button>{/snippet}</Alert>
    {:else if !history || !layout}
      <Skeleton rows={8} height={32} />
    {:else}
      <p class="history-summary" role="status">{describeHistory(history)}</p>
      <ul class="history-legend" aria-label="Legend">
        <li><svg width="14" height="14" aria-hidden="true"><circle class="dot" data-kind="local" cx="7" cy="7" r="5" /></svg>Local</li>
        <li><svg width="14" height="14" aria-hidden="true"><circle class="dot" data-kind="origin" cx="7" cy="7" r="5" /></svg>Origin</li>
        <li><svg width="14" height="14" aria-hidden="true"><circle class="dot" data-kind="uncommitted" cx="7" cy="7" r="5.5" /></svg>Uncommitted</li>
      </ul>
      {#if history.kind === 'noUpstream' || history.kind === 'detached'}
        <Alert kind="info" role="status">{history.kind === 'detached' ? 'HEAD is not on a branch, so there is no origin to compare with.' : 'This branch does not track an origin branch yet. Push it to start tracking.'}</Alert>
      {:else if !hasSharedBase(history)}
        <Alert kind="warn" role="status">Local and origin share no history, so the rails do not join.</Alert>
      {/if}
      {#if layout.rows.length}
        <HistoryGraph {layout} {shownId} bind:activeId />
        {#if canLoadMore(history)}
          <div class="history-more">
            {#if limit < LAST_PAGE}<button class="btn small" onclick={() => read(LAST_PAGE)}>Show up to {LAST_PAGE} per rail</button>
            {:else}<span class="mut">Showing the newest {LAST_PAGE} per rail.</span>{/if}
          </div>
        {/if}
      {:else}
        <EmptyState icon="commit" title="No commits yet" hint="Commit your first change to start this history." />
      {/if}
    {/if}
  </div>
  {#if history}<HistoryDetails row={active} />{/if}
</div>
