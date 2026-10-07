<script lang="ts">
  import type { SetItem } from '../../lib/api';
  import { detailsDrawer } from '../../lib/details-drawer.svelte';
  import { failureView, pullChip } from '../../lib/pull-chip';
  import { pullFlow, pullKey } from '../../lib/pull-flow.svelte';
  import { openPull, pulls } from '../../lib/pulls.svelte';
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';
  import PullChip from './PullChip.svelte';

  let { item, drawer = false }: { item: SetItem; drawer?: boolean } = $props();

  const key = $derived(pullKey(item));
  const path = $derived(key?.path);
  const branch = $derived(key?.branch);
  const entry = $derived(key ? pulls.entry(key) : undefined);
  const pull = $derived(entry?.status === 'ready' ? entry.pull : null);
  const live = $derived(!!pull && (pull.state === 'open' || pull.state === 'draft'));
  const busy = $derived(app.running || app.gitBusy || app.clonePreparing);

  $effect(() => {
    if (path !== undefined && branch !== undefined) return pulls.want({ path, branch });
  });
</script>

{#if key}
  <section class="pull-section" aria-label="Pull request">
    <h4>Pull request</h4>
    {#if pull}
      <PullChip view={pullChip(pull)} onopen={() => void openPull(pull.url)} />
      {#if drawer}<button class="pull-title link-title" title="Show the pull request details" onclick={event => detailsDrawer.open({ kind: 'pull', name: app.folderOf(item), pull }, event.currentTarget)}>{pull.title}</button>
      {:else}<p class="pull-title">{pull.title}</p>{/if}
      <p class="mut pull-meta">Into <span class="mono">{pull.base}</span> on <span class="mono">{pull.targetRepo}</span></p>
    {:else if entry?.status === 'failed'}
      <p class="warn" role="status">{failureView(entry.message).title}</p>
    {:else if entry?.status === 'ready'}
      <p class="mut">No pull request for <span class="mono">{key.branch}</span>.</p>
    {:else if pulls.limit}
      <p class="warn" role="status">{pulls.limit.message}</p>
    {:else}
      <p class="mut" role="status">Loading…</p>
    {/if}
    {#if !live}<button class="btn small" disabled={busy} onclick={event => pullFlow.openFor(item, event.currentTarget)}><Icon name="branch" tone="sync" /> Open pull request…</button>{/if}
  </section>
{/if}
