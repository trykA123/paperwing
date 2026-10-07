<script lang="ts">
  import { pullChip, failureView } from '../../lib/pull-chip';
  import type { PullKey } from '../../lib/pull-support';
  import { formatReset } from '../../lib/pull-support';
  import { openPull, pulls } from '../../lib/pulls.svelte';
  import { app } from '../../lib/state.svelte';
  import PullChip from './PullChip.svelte';

  let { target }: { target: PullKey | null } = $props();

  const path = $derived(target?.path);
  const branch = $derived(target?.branch);

  $effect(() => {
    if (path !== undefined && branch !== undefined) return pulls.want({ path, branch });
  });

  const entry = $derived(target ? pulls.entry(target) : undefined);

  async function open(url: string) {
    try { await openPull(url); }
    catch { app.toast('Could not open the browser. Copy the link from the details panel.', 'error'); }
  }
</script>

<span class="pull-cell">
  {#if !target}
    <span class="pull-none" aria-hidden="true">·</span>
  {:else if entry?.status === 'ready' && entry.pull}
    {@const pull = entry.pull}
    <PullChip view={pullChip(pull)} onopen={() => open(pull.url)} />
  {:else if entry?.status === 'ready'}
    <span class="pull-none" title="No pull request for this branch">None</span>
  {:else if entry?.status === 'failed'}
    {@const failure = failureView(entry.message)}
    <span class="pull-part warn" title={failure.title}>{failure.label}<span class="sr-only">: {failure.title}</span></span>
  {:else if pulls.limit}
    <span class="pull-part warn" title={pulls.limit.message}>Paused · {formatReset(pulls.limit.resetAt)}<span class="sr-only">: {pulls.limit.message}</span></span>
  {:else}
    <span class="pull-none">Loading…</span>
  {/if}
</span>
