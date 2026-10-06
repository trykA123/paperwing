<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import type { SetCompareState } from '../lib/compare.svelte';
  import CompareReferencePicker from './CompareReferencePicker.svelte';
  import Icon from './Icon.svelte';
  let { comparison }: { comparison: SetCompareState } = $props();
  let differences = $state(false), preparing = $state(false), error = $state('');
  const owner = $derived(app.ws.sets.find(set => set.id === comparison.setId));
  const ready = $derived(comparison.rows.filter(row => row.state === 'ready'));
  const rows = $derived(comparison.rows.filter(row => !differences || !row.snapshot || row.snapshot.display.different + row.snapshot.display.leftOnly + row.snapshot.display.rightOnly + row.snapshot.display.typeConflict + row.snapshot.display.unavailable > 0));
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
  <div class="set-compare-table"><table><thead><tr><th>Repository folder</th><th>Result</th><th>Different</th><th>Only left</th><th>Only right</th><th>Lines + / -</th><th>Ahead / behind</th></tr></thead><tbody>
    {#each rows as row (row.itemId)}<tr><td><button class="set-result" disabled={comparison.stale || !row.snapshot || row.state !== 'ready'} onclick={() => app.openSetCompareRow(row)}>{row.folder}</button></td>
      <td>{row.snapshot ? row.snapshot.display.unavailable ? 'Partly unavailable' : row.snapshot.display.different + row.snapshot.display.leftOnly + row.snapshot.display.rightOnly + row.snapshot.display.typeConflict ? 'Different' : 'Identical' : row.state}{#if row.message}<div class="warn">{row.message}</div>{/if}</td>
      <td>{row.snapshot ? row.snapshot.display.different + row.snapshot.display.typeConflict : '-'}</td><td>{row.snapshot?.display.leftOnly ?? '-'}</td><td>{row.snapshot?.display.rightOnly ?? '-'}</td>
      <td>{#if row.added === null}N/A{:else}<span class="set-added">+{row.added}</span> / <span class="set-removed">-{row.removed}</span>{/if}</td><td>{row.snapshot?.history.available ? `${row.snapshot.history.rightCount} / ${row.snapshot.history.leftCount}` : 'N/A'}</td></tr>{/each}
  </tbody></table></div>
</section>