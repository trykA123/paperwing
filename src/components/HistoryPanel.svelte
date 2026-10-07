<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type RepositoryHistory, type TagInfo } from '../lib/api';
  import { canLoadMore, describeHistory, hasSharedBase, layoutHistory, type GraphRow } from '../lib/history-graph';
  import { tagFlow } from '../lib/tag-flow.svelte';
  import Alert from './Alert.svelte';
  import EmptyState from './EmptyState.svelte';
  import HistoryDetails from './HistoryDetails.svelte';
  import HistoryGraph from './HistoryGraph.svelte';
  import Skeleton from './Skeleton.svelte';

  const NOTE = {
    detached: 'HEAD is not on a branch, so there is no origin to compare with. Showing commits that are not on any remote.',
    noUpstream: 'This branch does not track an origin branch yet. Showing commits that are not on any remote.',
    upstreamGone: 'The tracked branch was deleted or renamed on the remote. Showing commits that are not on any remote.',
  } as const;
  const FIRST_PAGE = 50;
  const LAST_PAGE = 200;
  type Load = { status: 'loading' } | { status: 'ready'; history: RepositoryHistory } | { status: 'error'; message: string };

  let { path, active = $bindable(null), inline = false, onloaded }: { path: string; active?: GraphRow | null; inline?: boolean; onloaded?: () => void } = $props();
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
  const shown = $derived(layout?.rows.find(row => row.id === shownId) ?? null);
  $effect(() => { active = shown; });

  async function read(next: number) {
    if (busy) return;
    busy = true;
    try {
      const result = await api.repositoryHistory(path, next);
      if (alive) { load = { status: 'ready', history: result }; limit = next; }
    } catch (error) {
      if (alive) load = { status: 'error', message: String(error) };
    } finally {
      busy = false;
    }
    onloaded?.();
  }

  async function readTags() {
    try { const next = await api.listTags(path); if (alive) tags = next; } catch { tags = []; }
  }

  $effect(() => { void tagFlow.revision; void readTags(); });
  onMount(() => { void read(FIRST_PAGE); return () => { alive = false; }; });
</script>

<div class="history-panel">
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
    {#if inline}<HistoryDetails row={shown} />{/if}
  {/if}
</div>
