<script lang="ts" module>
  export type Scope = 'selected' | 'set';
</script>

<script lang="ts">
  import type { SetItem } from '../../lib/api';
  import { app } from '../../lib/state.svelte';

  let { scope = $bindable(), selected, cloned, skipped, refs, setId, disabled }: {
    scope: Scope; selected: SetItem[]; cloned: SetItem[]; skipped: number; refs: Record<string, string>; setId: string; disabled: boolean;
  } = $props();
  const shown = $derived(scope === 'selected' ? selected : cloned);
</script>

<div class="cs-scope">
  <div class="seg small" role="radiogroup" aria-label="Where to search">
    <button type="button" role="radio" aria-checked={scope === 'selected'} class:on={scope === 'selected'} disabled={disabled || !selected.length} onclick={() => (scope = 'selected')}>Selected ({selected.length})</button>
    <button type="button" role="radio" aria-checked={scope === 'set'} class:on={scope === 'set'} disabled={disabled} onclick={() => (scope = 'set')}>Whole set ({cloned.length})</button>
  </div>
  {#if skipped}<span class="mut">{skipped} not cloned, skipped</span>{/if}
  <details class="cs-refs">
    <summary>Git ref per repository</summary>
    <p class="mut">Leave empty to search the working tree. Enter a branch, tag or commit to search that revision.</p>
    <ul>
      {#each shown as item (item.id)}
        <li><span class="mono" title={app.dest(item, setId)}>{app.folderOf(item)}</span>
          <input aria-label="Ref for {app.folderOf(item)}" placeholder="working tree" spellcheck="false" autocomplete="off" {disabled} bind:value={refs[app.dest(item, setId)]} /></li>
      {/each}
    </ul>
  </details>
</div>
