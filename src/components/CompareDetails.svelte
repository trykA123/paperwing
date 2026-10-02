<script lang="ts">
  import type { CompareState } from '../lib/compare.svelte';
  import type { View } from '../lib/workspace';
  import { app } from '../lib/state.svelte';
  import { detailFile, directory, sizeLabel, statusLabel } from '../lib/compare-view';
  import Icon from './Icon.svelte';
  let { comparison, comparisonId }: { comparison: CompareState; comparisonId: string } = $props();
  const selected = $derived(detailFile(app.view, comparisonId, comparison.files, comparison.selectedId, comparison.snapshot));
  async function apply() { await comparison.refresh($state.snapshot(comparison.options)); await comparison.loadAllFiles(); if (comparison.mode === 'commits') await comparison.loadAllCommits(); }
  function open() {
    const snapshot = comparison.snapshot;
    if (!selected || !snapshot || directory(selected)) return;
    const view: View = { kind: 'fileDiff', comparisonId, fileId: selected.id, path: selected.path, sessionId: snapshot.id, generation: snapshot.generation };
    app.openView(view);
  }
  function resize(event: PointerEvent) {
    const button = event.currentTarget as HTMLElement;
    button.setPointerCapture(event.pointerId);
    const move = (pointer: PointerEvent) => app.ws.rightWidth = Math.max(280, Math.min(520, innerWidth - pointer.clientX));
    const end = () => { button.removeEventListener('pointermove', move); button.removeEventListener('pointerup', end); };
    button.addEventListener('pointermove', move); button.addEventListener('pointerup', end);
  }
</script>
<section class="right compare-details">
  <button class="rresize" aria-label="Resize comparison details" title="Resize comparison details" onpointerdown={resize} onkeydown={event => { if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') app.ws.rightWidth = Math.max(280, Math.min(520, app.ws.rightWidth + (event.key === 'ArrowLeft' ? 20 : -20))); }}></button>
  <div class="rscroll">
    {#if app.view.kind === 'fileDiff' && !selected}<section><h3>Selected</h3><p class="mono compare-selected-path">{app.view.path}</p><p class="mut">This preview is stale. Reopen it from the folder comparison.</p></section>{/if}
    {#if selected}<section><h3>Selected</h3><p class="mono compare-selected-path">{selected.path}</p><dl class="compare-kv"><dt>Status</dt><dd class="compare-{selected.displayStatus}">{statusLabel[selected.displayStatus]}</dd><dt>Left</dt><dd>{sizeLabel(selected.left?.size)}</dd><dt>Right</dt><dd>{sizeLabel(selected.right?.size)}</dd>
      {#if selected.displayLines}<dt>Lines</dt><dd><span class="ok">+{selected.displayLines.added}</span> <span class="err">-{selected.displayLines.removed}</span></dd>{/if}</dl>
      {#if selected.reason}<p class="warn">{selected.reason}</p>{/if}
      {#if selected.rename}<p class="mut">Renamed from {selected.rename.from}</p>{/if}
      <button class="btn" disabled={directory(selected)} onclick={open}><Icon name="code" /> Open diff</button>
    </section>{/if}
    <section><h3>Comparison rules</h3><label class="compare-option"><input type="checkbox" checked disabled />Compare file contents</label>
      <label class="compare-option"><input type="checkbox" bind:checked={comparison.options.ignoreWhitespace} />Ignore whitespace</label>
      <label class="compare-option"><input type="checkbox" bind:checked={comparison.options.normalizeEol} />Ignore line endings (CRLF / LF)</label>
      <button class="btn" disabled={comparison.busy || !comparison.snapshot} onclick={apply}><Icon name="refresh" /> Apply rules</button>
      <label class="compare-exclude">Exclude<input class="mono" bind:value={comparison.excludes} /></label>
    </section>
    <section><h3>Copy</h3><div class="compare-copy-buttons"><button class="btn" disabled title="Available after the recovery service"><Icon name="copy" /> To left</button><button class="btn" disabled title="Available after the recovery service"><Icon name="copy" /> To right</button></div></section>
  </div>
</section>