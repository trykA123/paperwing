<script lang="ts">
  import type { DrawerTarget } from '../../lib/details-drawer.svelte';
  import { app } from '../../lib/state.svelte';
  import HistoryDetails from '../HistoryDetails.svelte';
  import Icon from '../Icon.svelte';

  let { target }: { target: Extract<DrawerTarget, { kind: 'commit' }> } = $props();

  const sha = $derived(target.row.commit?.sha ?? '');
  const copy = () => { void navigator.clipboard.writeText(sha); app.toast('Commit id copied', 'success'); };
</script>

<div class="history-body">
  <HistoryDetails row={target.row} />
  {#if sha}<button class="btn small" onclick={copy}><Icon name="copy" />Copy commit id</button>{/if}
</div>
