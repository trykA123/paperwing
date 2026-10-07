<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { fade, fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { api, type RepositoryHistory, type TagInfo } from '../lib/api';
  import { app } from '../lib/state.svelte';
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
  import StashSection from './stash/StashSection.svelte';
  import TagSection from './tags/TagSection.svelte';

  const NOTE = {
    detached: 'HEAD is not on a branch, so there is no origin to compare with. Showing commits that are not on any remote.',
    noUpstream: 'This branch does not track an origin branch yet. Showing commits that are not on any remote.',
    upstreamGone: 'The tracked branch was deleted or renamed on the remote. Showing commits that are not on any remote.',
  } as const;
  const FIRST_PAGE = 50;
  const LAST_PAGE = 200;
  type Load = { status: 'loading' } | { status: 'ready'; history: RepositoryHistory } | { status: 'error'; message: string };

  let { target }: { target: HistoryTarget } = $props();
  let panel: HTMLElement;
  let load = $state<Load>({ status: 'loading' });
  let limit = $state(FIRST_PAGE);
  let activeId = $state<string | null>(null);
  let busy = $state(false);
  let tags = $state<TagInfo[]>([]);
  let alive = true;

  const history = $derived(load.status === 'ready' ? load.history : null);
  const layout = $derived(history ? layoutHistory(history, tags) : null);
  const defaultId = $derived((layout?.rows.find(row => row.commit) ?? layout?.rows[0])?.id ?? null);
  const shownId = $derived(layout?.rows.some(row => row.id === activeId) ? activeId : defaultId);
  const active = $derived(layout?.rows.find(row => row.id === shownId) ?? null);

  function focusPanel(tries = 10) {
    if (!alive || panel.contains(document.activeElement)) return;
    panel.focus();
    if (document.activeElement !== panel && tries > 0) requestAnimationFrame(() => focusPanel(tries - 1));
  }

  async function read(next: number) {
    if (busy) return;
    busy = true;
    try {
      const result = await api.repositoryHistory(target.path, next);
      if (alive) { load = { status: 'ready', history: result }; limit = next; }
    } catch (error) {
      if (alive) load = { status: 'error', message: String(error) };
    } finally {
      busy = false;
    }
    await tick();
    focusPanel();
  }

  onMount(() => {
    const opener = historyDrawer.opener;
    focusPanel();
    void read(FIRST_PAGE);
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
      {#if history}<span class="history-branch mono"><Icon name="branch" tone="branch" />{history.branch ?? 'detached HEAD'}</span>{/if}
    </div>
    <button class="btn small" title="Delete branches that are already merged" onclick={() => { app.cleanupDialog ??= { targets: [{ path: target.path, name: target.name }] }; }}><Icon name="trash" />Clean up branches</button>
    <button class="shell-control" title="Close history" aria-label="Close history" onclick={() => historyDrawer.close()}><Icon name="close" /></button>
  </header>
  <div class="history-body">
    {#if load.status === 'error'}
      <Alert kind="err" role="alert">{load.message}{#snippet action()}<button class="btn small" aria-disabled={busy} onclick={() => read(limit)}>{busy ? 'Retrying…' : 'Retry'}</button>{/snippet}</Alert>
    {:else if !history || !layout}
      <Skeleton rows={8} height={32} />
    {:else}
      <p class="history-summary" role="status">{describeHistory(history)}</p>
      <ul class="history-legend" aria-label="Legend">
        <li><svg width="14" height="14" aria-hidden="true"><circle class="dot" data-kind="local" cx="7" cy="7" r="5" /></svg>Local</li>
        <li><svg width="14" height="14" aria-hidden="true"><circle class="dot" data-kind="origin" cx="7" cy="7" r="5" /></svg>Origin</li>
        <li><svg width="14" height="14" aria-hidden="true"><circle class="dot" data-kind="uncommitted" cx="7" cy="7" r="5.5" /></svg>Uncommitted</li>
      </ul>
      {#if history.kind === 'noUpstream' || history.kind === 'detached' || history.kind === 'upstreamGone'}
        <Alert kind="info" role="status">{NOTE[history.kind]}</Alert>
      {:else if !hasSharedBase(history)}
        <Alert kind="warn" role="status">Local and origin share no history, so the rails do not join.</Alert>
      {/if}
      <TagSection path={target.path} name={target.name} bind:tags />
      <StashSection path={target.path} name={target.name} />
      {#if layout.rows.length}
        <HistoryGraph {layout} {shownId} bind:activeId />
        {#if canLoadMore(history)}
          <div class="history-more">
            {#if limit < LAST_PAGE}<button class="btn small" aria-disabled={busy} onclick={() => read(LAST_PAGE)}>{busy ? 'Loading…' : `Show up to ${LAST_PAGE} per rail`}</button>
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
