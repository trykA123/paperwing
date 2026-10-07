<script lang="ts">
  import { onMount } from 'svelte';
  import type { DrawerTarget } from '../../lib/details-drawer.svelte';
  import StashSection from '../stash/StashSection.svelte';

  let { target, onready }: { target: Extract<DrawerTarget, { kind: 'stash' }>; onready: () => void } = $props();

  onMount(() => onready());
</script>

<div class="history-body"><StashSection path={target.path} name={target.name} only={target.oid} /></div>
