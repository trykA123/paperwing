<script lang="ts">
  import type { Command } from '../../lib/commands';
  import Icon from '../Icon.svelte';

  let { path, count, previous, next, inline = $bindable(), hideSame = $bindable(), ignoreWhitespace = $bindable(), onexecute }: {
    path: string; count: string; previous: Command; next: Command;
    inline: boolean; hideSame: boolean; ignoreWhitespace: boolean; onexecute: (command: Command) => void;
  } = $props();
</script>

<header class="compare-summary editor-toolbar"><strong class="mono">{path}</strong><span class="grow"></span>
    <button class="btn small icon-only flip" title="Previous difference (Shift+F7)" aria-label="Previous difference" disabled={!previous.enabled} onclick={() => onexecute(previous)}><Icon name="chevron" /></button>
    <span class="editor-count">{count}</span>
    <button class="btn small icon-only" title="Next difference (F7)" aria-label="Next difference" disabled={!next.enabled} onclick={() => onexecute(next)}><Icon name="chevron" /></button>
    <div class="seg small"><button class:on={!inline} onclick={() => inline = false}>Side by side</button><button class:on={inline} onclick={() => inline = true}>Inline</button></div>
    <label class="check"><input type="checkbox" bind:checked={hideSame} /> Hide unchanged</label>
    <label class="check"><input type="checkbox" bind:checked={ignoreWhitespace} /> Ignore whitespace</label>
  </header>
