<script lang="ts">
  import { nextRowIndex, ringLabel, ringRadius, ROW_HEIGHT, type GraphLayout, type GraphRow } from '../lib/history-graph';
  import Icon from './Icon.svelte';

  const TAGS_SHOWN = 2;
  let { layout, shownId, activeId = $bindable(null), onpick }: { layout: GraphLayout; shownId: string | null; activeId?: string | null; onpick?: (row: GraphRow, opener: Element) => void } = $props();

  let list: HTMLUListElement;
  let focusedId = $state<string | null>(null);
  const rows = $derived(layout.rows);
  const stopId = $derived(rows.some(row => row.id === focusedId) ? focusedId : rows[0]?.id);

  function move(event: KeyboardEvent, index: number) {
    const target = nextRowIndex(event.key, index, rows.length);
    if (target === null) return;
    event.preventDefault();
    list.querySelectorAll<HTMLButtonElement>('.history-row')[target]?.focus();
  }
</script>

<div class="history-graph" style:--graph-w="{layout.width}px" style:--row-h="{ROW_HEIGHT}px">
  <svg class="history-rails" width={layout.width} height={layout.height} viewBox="0 0 {layout.width} {layout.height}" aria-hidden="true">
    {#each layout.paths as path (path.id)}<path class="rail" data-rail={path.rail} d={path.d} />{/each}
    {#each rows as row (row.id)}
      {#if row.kind === 'uncommitted'}
        <circle class="dot" data-kind="uncommitted" class:active={row.id === shownId} cx={row.x} cy={row.y} r={ringRadius(row.count)} />
        <text class="wt-count" x={row.x} y={row.y} text-anchor="middle" dominant-baseline="central">{ringLabel(row.count)}</text>
      {:else if row.kind === 'more'}
        <circle class="dot" data-kind="more" cx={row.x} cy={row.y} r="2" />
      {:else}
        <circle class="dot" data-kind={row.kind} class:active={row.id === shownId} cx={row.x} cy={row.y} r={row.kind === 'below' ? 4 : 5} />
      {/if}
    {/each}
  </svg>
  <ul class="history-list" bind:this={list} aria-label="Commits">
    {#each rows as row, index (row.id)}
      <li>
        <button class="history-row" data-kind={row.kind} class:on={row.id === shownId} tabindex={row.id === stopId ? 0 : -1}
          onfocus={() => { focusedId = row.id; activeId = row.id; }} onblur={() => { if (focusedId === row.id) focusedId = null; }}
          onpointerenter={() => (activeId = row.id)} onpointerleave={() => { if (activeId === row.id) activeId = focusedId; }}
          onkeydown={event => move(event, index)} onclick={event => { if (row.commit) onpick?.(row, event.currentTarget); }}>
          {#if row.commit}<span class="history-sha">{row.commit.short}</span>{/if}
          <span class="history-label">{row.label}</span>
          {#each row.tags.slice(0, TAGS_SHOWN) as tag (tag)}<span class="history-tagref" title="Tag {tag}"><Icon name="tag" size={10} tone="tag" />{tag}</span>{/each}
          {#if row.tags.length > TAGS_SHOWN}<span class="history-tagref" title={row.tags.slice(TAGS_SHOWN).join(', ')}>+{row.tags.length - TAGS_SHOWN}</span>{/if}
          {#if row.tag}<span class="history-tag" data-kind={row.kind}>{row.tag}</span>{/if}
        </button>
      </li>
    {/each}
  </ul>
</div>
