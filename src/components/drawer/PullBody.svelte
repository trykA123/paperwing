<script lang="ts">
  import { pullChip } from '../../lib/pull-chip';
  import type { DrawerTarget } from '../../lib/details-drawer.svelte';
  import { openPull } from '../../lib/pulls.svelte';
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';
  import PullChip from '../pulls/PullChip.svelte';

  let { target }: { target: Extract<DrawerTarget, { kind: 'pull' }> } = $props();

  const pull = $derived(target.pull);
  const open = () => openPull(pull.url).catch(() => app.toast('Could not open the browser. Copy the link instead.', 'error'));
</script>

<div class="history-body">
  <PullChip view={pullChip(pull)} onopen={() => void open()} />
  <dl class="drawer-facts">
    <dt>Into</dt><dd class="mono">{pull.base}</dd>
    <dt>Repository</dt><dd class="mono">{pull.targetRepo}</dd>
    <dt>State</dt><dd>{pull.state}</dd>
    <dt>Review</dt><dd>{pull.reviewState}</dd>
    <dt>Checks</dt><dd>{pull.checks.total ? `${pull.checks.success} passed, ${pull.checks.failure} failed, ${pull.checks.pending} running` : 'None'}</dd>
  </dl>
  <button class="btn small dark" onclick={() => void open()}><Icon name="external" />Open on {new URL(pull.url).hostname}</button>
</div>
