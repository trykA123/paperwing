<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { CompareSnapshot } from '../../lib/api';
  import type { Command } from '../../lib/commands';
  import { refLabel } from '../../lib/compare-view';
  import Icon from '../Icon.svelte';

  type Endpoint = CompareSnapshot['left'];
  let { snapshot, path, count, previous, next, saveRight, onexecute, onback, actions }: {
    snapshot: CompareSnapshot | null; path: string; count: string; previous: Command; next: Command; saveRight: Command;
    onexecute: (command: Command) => void; onback?: () => void; actions?: Snippet;
  } = $props();
  const slash = $derived(path.lastIndexOf('/') + 1);
  const detail = (endpoint: Endpoint) => (endpoint.endpoint.reference.kind === 'workingTree' ? '' : endpoint.commit.slice(0, 7));
</script>

{#snippet pill(side: string, endpoint: Endpoint)}
  <span class="fc-ref"><i>{side}</i><b title={refLabel(endpoint.endpoint.reference)}>{refLabel(endpoint.endpoint.reference)}</b>{#if detail(endpoint)}<i>{detail(endpoint)}</i>{/if}</span>
{/snippet}

<header class="fc-head">
  {#if onback}<button class="btn fc-back" title="Back (Esc)" onclick={onback}><Icon name="arrowLeft" size={16} />Back<kbd>Esc</kbd></button>{/if}
  {#if snapshot}<div class="fc-refs" role="group" aria-label="Compared references">{@render pill('left', snapshot.left)}<span class="fc-swap" aria-hidden="true"><Icon name="swap" size={16} /></span>{@render pill('right', snapshot.right)}</div>{/if}
  <div class="fc-path mono" title={path}><span>{path.slice(0, slash)}</span>{path.slice(slash)}</div>
  <div class="fc-nav">
    <button class="btn icon-only fc-prev" title="Previous change (P, Shift+F7)" aria-label="Previous change" disabled={!previous.enabled} onclick={() => onexecute(previous)}><Icon name="chevron" size={16} /></button>
    <span class="editor-count" role="status">{count}</span>
    <button class="btn icon-only fc-next" title="Next change (N, F7)" aria-label="Next change" disabled={!next.enabled} onclick={() => onexecute(next)}><Icon name="chevron" size={16} /></button>
    <kbd aria-hidden="true">P</kbd><kbd aria-hidden="true">N</kbd>
  </div>
  {@render actions?.()}
  <button class="btn dark fc-save" title={saveRight.reason ?? 'Save the right file (Ctrl+S saves every changed file)'} disabled={!saveRight.enabled} onclick={() => onexecute(saveRight)}><Icon name="save" size={16} /><span class="fc-label">Save right</span></button>
</header>
