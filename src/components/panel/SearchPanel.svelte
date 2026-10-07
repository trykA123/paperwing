<script lang="ts">
  import type { SearchMode } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import { tabId } from '../../lib/workspace';
  import Icon from '../Icon.svelte';

  const MODES: { id: SearchMode; label: string }[] = [
    { id: 'fixed', label: 'Plain text' }, { id: 'basic', label: 'Regular expression' }, { id: 'perl', label: 'Perl regular expression' },
  ];
  const session = $derived(app.codeSearches[tabId({ kind: 'codeSearch' }, app.set.id)]);

  function reuse(pattern: string) {
    session.form.pattern = pattern;
    document.querySelector<HTMLInputElement>('.cs-pattern input')?.focus();
  }
</script>

{#if session}
  <div class="sec">
    <h6>Mode</h6>
    {#each MODES.filter(mode => session.perl || mode.id !== 'perl') as mode (mode.id)}
      <button class="nav" class:on={session.form.mode === mode.id} aria-pressed={session.form.mode === mode.id} onclick={() => (session.form.mode = mode.id)}><span class="lbl">{mode.label}</span></button>
    {/each}
  </div>
  <div class="sec">
    <h6>Recent searches</h6>
    {#each session.recent as pattern (pattern)}
      <button class="nav" title="Use this pattern again" onclick={() => reuse(pattern)}><Icon name="search" /><span class="lbl">{pattern}</span></button>
    {:else}<div class="nav ghost">Nothing searched yet</div>{/each}
  </div>
{:else}
  <div class="sec">
    <h6>Search</h6>
    <button class="nav" onclick={() => app.openCodeSearch()}><Icon name="search" /><span class="lbl">Search code in {app.set.name}</span></button>
  </div>
{/if}
