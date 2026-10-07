<script lang="ts">
  import type { ChipPart, ChipView } from '../../lib/pull-chip';
  import Icon from '../Icon.svelte';

  let { view, onopen }: { view: ChipView; onopen: () => void } = $props();

  const parts = $derived([view.review, view.checks].filter((part): part is ChipPart => !!part));
  const spoken = $derived([`Pull request ${view.number}`, view.state.label, ...parts.map(part => part.title)].join(', '));
</script>

<button class="pull-chip" title="{view.title} · open in the browser" aria-label="{spoken}. Open in the browser" onclick={onopen}>
  <span class="pull-num">{view.number}</span>
  <span class="pull-part {view.state.tone}">{view.state.label}</span>
  {#each parts as part (part.title)}
    <span class="pull-part pull-extra {part.tone}" title={part.title}>{#if part.icon}<Icon name={part.icon} size={12} />{/if}<span class="pull-text">{part.label}</span></span>
  {/each}
</button>
