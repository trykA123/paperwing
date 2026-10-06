<script lang="ts">
  import type { SetItem } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';

  let { items }: { items: SetItem[] } = $props();

  const jobs = $derived(items.map(item => app.jobs[item.id]).filter(Boolean));
  const done = $derived(jobs.filter(job => job.phase === 'done' || job.phase === 'skipped').length);
  const failed = $derived(jobs.filter(job => job.phase === 'failed').length);
  const missing = $derived(items.filter(item => app.refState(item) === 'missing').length);
</script>

<div class="rfoot">
  <div class="sum">
    {#if app.running}{done + failed} of {items.length} finished{#if failed} · <b>{failed} failed</b>{/if}
    {:else}<span>{items.length} to clone</span>{#if missing}<b>{missing} with a missing ref</b>{/if}{/if}
  </div>
  <button class="btn dark go" title={app.rootSupport.reason ?? 'Clone repositories'} disabled={app.running || app.clonePreparing || !items.length || !app.rootSupport.valid} onclick={() => app.startClone(items)}>
    {#if app.running}<span class="spin"></span> Cloning…{:else}<Icon name="folder" /> Clone {items.length} repos{/if}
  </button>
</div>
