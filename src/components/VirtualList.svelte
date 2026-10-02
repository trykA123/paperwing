<script lang="ts" generics="T">
  import type { Snippet } from 'svelte';

  let { items, rowHeight, key, row, header, empty }: {
    items: T[];
    rowHeight: number;
    key: (item: T) => string;
    row: Snippet<[T]>;
    header?: Snippet;
    empty?: Snippet;
  } = $props();

  const OVERSCAN = 6;
  let box: HTMLDivElement;
  let scrollTop = $state(0);
  let viewH = $state(0);
  let headH = $state(0);

  // Only rows inside the viewport (+ overscan) are in the DOM; spacers keep the scrollbar honest.
  const top = $derived(Math.max(0, scrollTop - headH));
  const start = $derived(Math.min(Math.max(0, items.length - 1), Math.max(0, Math.floor(top / rowHeight) - OVERSCAN)));
  const end = $derived(Math.min(items.length, Math.ceil((top + (viewH || 800)) / rowHeight) + OVERSCAN));
  const visible = $derived(items.slice(start, end));
  $effect(() => {
    const maximum = Math.max(0, headH + items.length * rowHeight - viewH);
    if (box && box.scrollTop > maximum) { box.scrollTop = maximum; scrollTop = maximum; }
  });
</script>

<div class="vbox" bind:this={box} bind:clientHeight={viewH} onscroll={() => (scrollTop = box.scrollTop)}>
  {#if header}<div class="vhead" bind:offsetHeight={headH}>{@render header()}</div>{/if}
  {#if items.length === 0 && empty}{@render empty()}{/if}
  <div style:height="{start * rowHeight}px"></div>
  {#each visible as item (key(item))}
    <div class="vrow" style:height="{rowHeight}px">{@render row(item)}</div>
  {/each}
  <div style:height="{(items.length - end) * rowHeight}px"></div>
</div>
