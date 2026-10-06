<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import type { SetCompareState } from '../lib/compare.svelte';
  import CompareReferencePicker from './CompareReferencePicker.svelte';
  import Icon from './Icon.svelte';
  import { RESULT_GLYPH, hasDifferences, nextSort, rowView, sortRows, type Sort, type SortKey } from '../lib/set-compare-view';

  const COLUMNS: { key: SortKey; label: string; num?: boolean }[] = [
    { key: 'folder', label: 'Repository folder' }, { key: 'result', label: 'Result' }, { key: 'different', label: 'Different', num: true },
    { key: 'leftOnly', label: 'Only left', num: true }, { key: 'rightOnly', label: 'Only right', num: true },
    { key: 'lines', label: 'Lines + / \u2212', num: true }, { key: 'history', label: 'Ahead / behind', num: true },
  ];
  const UNKNOWN = '\u2013';
  let { comparison }: { comparison: SetCompareState } = $props();
  let differences = $state(true), sort = $state<Sort | null>(null), preparing = $state(false), error = $state('');
  const owner = $derived(app.ws.sets.find(set => set.id === comparison.setId));
  const ready = $derived(comparison.rows.filter(row => row.state === 'ready'));
  const rows = $derived(sortRows(comparison.rows.filter(row => !differences || hasDifferences(row)), sort));
  const total = $derived(ready.reduce((total, row) => total + row.snapshot!.display.different + row.snapshot!.display.leftOnly + row.snapshot!.display.rightOnly + row.snapshot!.display.typeConflict, 0));
  let fingerprint = '';
  let mounted = false;
  const paths = $derived(owner?.items.map(item => app.dest(item, comparison.setId)) ?? []);
  function context() { return JSON.stringify([app.ws.root, app.ws.layout, app.ws.pathTemplate, owner?.name, owner?.items.map(item => [item.id, item.folder, item.name, item.url, item.ref]), comparison.left, comparison.right, comparison.options]); }
  async function compare() {
    if (!mounted || !owner || preparing) return;
    preparing = true; error = '';
    try {
      await api.saveSettings({ sources: $state.snapshot(app.sources), workspace: $state.snapshot(app.ws) });
      if (!mounted || !owner) return;
      fingerprint = context(); await comparison.run($state.snapshot(owner.items));
    }
    catch (reason) { error = String(reason); } finally { preparing = false; }
  }
  $effect(() => {
    const current = context();
    if (fingerprint && current !== fingerprint) { fingerprint = current; comparison.stale = true; void comparison.cancel().catch(reason => error = String(reason)); }
  });
  onMount(() => { mounted = true; void compare(); return () => { mounted = false; void comparison.cancel().catch(() => {}); }; });
</script>

<section class="set-compare folder-compare">
  <header class="mh"><div class="grow"><div class="crumb">Whole set</div><h1>{owner?.name ?? 'Removed set'}</h1><div class="mut">{comparison.rows.length} repository folders</div></div>
    <button class="btn" disabled={preparing || comparison.busy || !owner} onclick={compare}><Icon name="refresh" /> Compare</button>
    <button class="btn" disabled={!comparison.busy} onclick={() => comparison.cancel().catch(reason => error = String(reason))}><Icon name="close" /> Cancel</button></header>
  <div class="compare-bar"><div class="compare-sides">{#each ['left', 'right'] as sideValue}{@const side = sideValue as 'left' | 'right'}
    <div class="compare-endpoint"><span class="endpoint-side">{side.toUpperCase()}</span>
      <CompareReferencePicker bind:reference={comparison[side]} {paths} label="Common {side}" disabled={comparison.busy || preparing} />
    </div>{/each}</div></div>
  <div class="compare-filters"><label><input type="checkbox" bind:checked={differences} /> Differences only</label><label><input type="checkbox" bind:checked={comparison.options.normalizeEol} disabled={comparison.busy} /> Normalize EOL</label><label><input type="checkbox" bind:checked={comparison.options.ignoreWhitespace} disabled={comparison.busy} /> Ignore whitespace</label><span class="grow"></span><span class="faint">All files · no exclusions</span></div>
  <div class="compare-summary"><b>{comparison.rows.filter(row => !['queued', 'comparing'].includes(row.state)).length} / {comparison.rows.length} completed</b><span>{ready.length} comparable</span><span>{total} changed files</span><span>{comparison.rows.filter(row => !['ready', 'queued', 'comparing'].includes(row.state)).length} unavailable or cancelled</span></div>
  {#if comparison.stale}<p class="warn">Set context changed. Compare again before opening results.</p>{/if}{#if error}<p class="warn" role="alert">{error}</p>{/if}
  <div class="set-compare-table"><table><thead><tr>
    {#each COLUMNS as column (column.key)}
      <th class:num={column.num} aria-sort={sort?.key === column.key ? (sort.dir === 'asc' ? 'ascending' : 'descending') : 'none'}>
        <button class="sort-head" onclick={() => (sort = nextSort(sort, column.key))}>{column.label}<span class="sort-mark" aria-hidden="true">{sort?.key === column.key ? (sort.dir === 'asc' ? '\u2191' : '\u2193') : ''}</span></button>
      </th>
    {/each}
  </tr></thead><tbody>
    {#each rows as row (row.itemId)}
      {@const view = rowView(row)}
      <tr><td><button class="set-result" disabled={comparison.stale || !row.snapshot || row.state !== 'ready'} onclick={() => app.openSetCompareRow(row)}>{row.folder}</button></td>
        <td><span class="cmp-chip k-{view.kind}">{#if RESULT_GLYPH[view.kind]}<b aria-hidden="true">{RESULT_GLYPH[view.kind]}</b>{/if}{view.label}</span>{#if row.message}<div class="warn">{row.message}</div>{/if}</td>
        <td class="num">{view.different ?? UNKNOWN}</td><td class="num">{view.leftOnly ?? UNKNOWN}</td><td class="num">{view.rightOnly ?? UNKNOWN}</td>
        <td class="num">{#if row.added === null}N/A{:else}<span class="set-added">+{row.added}</span> <span class="set-removed">−{row.removed}</span>{/if}</td>
        <td class="num">{row.snapshot?.history.available ? `${row.snapshot.history.rightCount} / ${row.snapshot.history.leftCount}` : 'N/A'}</td></tr>
    {/each}
  </tbody></table></div>
</section>