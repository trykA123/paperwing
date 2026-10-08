<script lang="ts">
  import type { GraphRow } from '../../lib/history-graph';
  import type { DrawerTarget } from '../../lib/details-drawer.svelte';
  import HistoryDetails from '../HistoryDetails.svelte';
  import HistoryPanel from '../HistoryPanel.svelte';
  import StashSection from '../stash/StashSection.svelte';
  import TagSection from '../tags/TagSection.svelte';

  let { target, onready }: { target: Extract<DrawerTarget, { kind: 'history' }>; onready: () => void } = $props();
  let active = $state<GraphRow | null>(null);
</script>

<div class="details-body">
  <TagSection path={target.path} name={target.name} />
  <StashSection path={target.path} name={target.name} />
  <HistoryPanel path={target.path} bind:active onloaded={onready} />
</div>
<HistoryDetails row={active} />
