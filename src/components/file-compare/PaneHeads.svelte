<script lang="ts">
  import type { CompareSnapshot, EditFile } from '../../lib/api';
  import type { Command } from '../../lib/commands';
  import { refLabel } from '../../lib/compare-view';
  import { formatLabel, type TextFormat } from '../../lib/text-format';
  import Icon from '../Icon.svelte';

  let { snapshot, tickets, formats, dirty, reasons = [], saveLeft, copyLeft, copyRight, oncopy, onexecute }: {
    snapshot: CompareSnapshot; tickets: (EditFile | null)[]; formats: TextFormat[]; dirty: boolean[]; reasons?: (string | null)[];
    saveLeft: Command; copyLeft: Command; copyRight: Command; oncopy: (side: 'left' | 'right') => void; onexecute: (command: Command) => void;
  } = $props();
  const sides = $derived([snapshot.left, snapshot.right]);
</script>

{#snippet pane(index: number)}
  {@const side = sides[index]!}
  <div class="fc-pane">
    <b class="mono" title={refLabel(side.endpoint.reference)}>{refLabel(side.endpoint.reference)}</b>
    {#if side.endpoint.reference.kind !== 'workingTree'}<span class="fc-dim mono">{side.commit.slice(0, 7)}</span>{/if}
    {#if !tickets[index]}<span class="fc-tag" title={reasons[index] ?? undefined}>Read-only</span>{/if}
    {#if dirty[index]}<span class="fc-tag fc-unsaved">Unsaved</span>{/if}
    <span class="grow"></span>
    <span class="fc-dim">{tickets[index] ? formatLabel(formats[index]) : ''}</span>
    {#if index === 0 && tickets[0]}<button class="btn small" title={saveLeft.reason ?? 'Save the left file'} disabled={!saveLeft.enabled} onclick={() => onexecute(saveLeft)}><Icon name="save" size={14} />Save left</button>{/if}
  </div>
{/snippet}

<div class="fc-panes editor-endpoints">
  {@render pane(0)}
  <div class="fc-mid">
    <button class="btn small icon-only" title={copyLeft.reason ?? 'Copy this change to the left file'} aria-label="Copy this change to the left file" disabled={!copyLeft.enabled} onclick={() => oncopy('left')}><Icon name="arrowLeft" size={16} /></button>
    <button class="btn small icon-only" title={copyRight.reason ?? 'Copy this change to the right file'} aria-label="Copy this change to the right file" disabled={!copyRight.enabled} onclick={() => oncopy('right')}><Icon name="arrowRight" size={16} /></button>
  </div>
  {@render pane(1)}
</div>
