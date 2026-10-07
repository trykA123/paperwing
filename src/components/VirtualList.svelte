<script lang="ts" generics="T">
  import type { Snippet } from 'svelte';

  let { items, rowHeight, key, row, header, empty, role, label, onkeydown, activeKey, initialScroll = 0, onscrolled, onvisible, keyshortcuts }: {
    items: T[];
    rowHeight: number;
    key: (item: T) => string;
    row: Snippet<[T, number]>;
    header?: Snippet;
    empty?: Snippet;
    role?: 'grid';
    label?: string;
    activeKey?: string;
    onkeydown?: (event: KeyboardEvent) => void;
    initialScroll?: number;
    onscrolled?: (top: number) => void;
    onvisible?: (items: T[]) => void;
    keyshortcuts?: string;
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
  const activeShown = $derived(activeKey === undefined || visible.some(item => key(item) === activeKey));
  export function reveal(index: number) {
    const above = index * rowHeight;
    if (above < box.scrollTop) box.scrollTop = above;
    else if (headH + above + rowHeight > box.scrollTop + viewH) box.scrollTop = headH + above + rowHeight - viewH;
    scrollTop = box.scrollTop;
  }

  $effect(() => { onvisible?.(visible); });

  let restored = false;
  $effect(() => {
    if (!box || restored || !initialScroll) return;
    restored = true;
    box.scrollTop = initialScroll;
    scrollTop = box.scrollTop;
  });

  $effect(() => {
    const maximum = Math.max(0, headH + items.length * rowHeight - viewH);
    if (box && box.scrollTop > maximum) { box.scrollTop = maximum; scrollTop = maximum; }
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div class="vbox" bind:this={box} bind:clientHeight={viewH} onscroll={() => { if (!box) return; scrollTop = box.scrollTop; onscrolled?.(scrollTop); }} {role} aria-label={label} aria-multiselectable={role === 'grid' ? true : undefined} aria-keyshortcuts={keyshortcuts} aria-rowcount={role === 'grid' ? items.length + (header ? 1 : 0) : undefined}
  tabindex={role === 'grid' && !activeShown ? 0 : undefined} {onkeydown}>
  {#if header}<div class="vhead" bind:offsetHeight={headH}>{@render header()}</div>{/if}
  {#if items.length === 0 && empty}{@render empty()}{/if}
  <div style:height="{start * rowHeight}px"></div>
  {#each visible as item, offset (key(item))}
    <div class="vrow" style:height="{rowHeight}px">{@render row(item, start + offset)}</div>
  {/each}
  <div style:height="{(items.length - end) * rowHeight}px"></div>
</div>
