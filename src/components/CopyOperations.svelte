<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type CopyPreview, type CopyOutcome } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import Icon from './Icon.svelte';
  import { dialogOut } from '../lib/motion';
  let { request }: { request: NonNullable<typeof app.copyRequest> } = $props();
  let dialog: HTMLDialogElement;
  let preview = $state<CopyPreview | null>(null), outcomes = $state<CopyOutcome[] | null>(null);
  let busy = $state(true), error = $state(''), disposed = false;
  let undone = $state<string[]>([]);
  async function close() {
    if (preview) await api.copyCancel(preview.id);
    if (!busy) app.copyRequest = null;
  }
  async function apply() {
    if (!preview || busy || !app.platform.capabilities.copy.supported) return;
    busy = true; error = '';
    try {
      const snapshot = app.comparisons[request.comparisonId]?.snapshot;
      if (!snapshot || snapshot.id !== request.id || snapshot.generation !== request.generation) throw new Error('Comparison changed. Reopen the copy preview.');
      await app.probeEndpoints([snapshot[request.side].endpoint]);
      const capability = app.endpointCapability(snapshot[request.side].endpoint, 'copy');
      if (!capability.supported) throw new Error(capability.reason ?? 'Copy is unavailable.');
      outcomes = await api.copyApply(preview.id, true);
      const comparison = app.comparisons[request.comparisonId];
      if (comparison?.snapshot?.id === request.id) { await comparison.refresh(); await comparison.loadAllFiles(); }
    } catch (reason) { error = String(reason); }
    finally { busy = false; }
  }
  async function undo() {
    if (!app.capability('recovery').supported || !outcomes || busy || !await app.prepareDiskMutation()) return;
    busy = true; error = '';
    try {
      for (const outcome of [...outcomes].reverse()) {
        if (!outcome.record || undone.includes(outcome.record.id)) continue;
        await api.recoveryUndo(outcome.record.id); undone.push(outcome.record.id);
      }
      const comparison = app.comparisons[request.comparisonId];
      if (comparison?.id) { await comparison.refresh(); await comparison.loadAllFiles(); }
    } catch (reason) { error = `Partial undo: ${String(reason)}. Completed restores are retained; later edits were not overwritten.`; }
    finally { busy = false; }
  }
  onMount(() => {
    dialog.showModal();
    (async () => {
      const snapshot = app.comparisons[request.comparisonId]?.snapshot;
      if (!snapshot || snapshot.id !== request.id || snapshot.generation !== request.generation) throw new Error('Comparison changed. Reopen the copy preview.');
      await app.probeEndpoints([snapshot[request.side].endpoint]);
      const capability = app.endpointCapability(snapshot[request.side].endpoint, 'copy');
      if (!capability.supported) throw new Error(capability.reason ?? 'Copy is unavailable.');
      if (!await app.prepareDiskMutation()) { app.copyRequest = null; return; }
      const result = await api.copyPreview(request.id, request.generation, request.fileId, request.side);
      if (disposed) { await api.copyCancel(result.id); return; }
      preview = result;
    })().catch(reason => { error = String(reason); }).finally(() => { busy = false; });
    return () => { disposed = true; if (preview) void api.copyCancel(preview.id).catch(() => {}); };
  });
</script>

<dialog class="operation-dialog" bind:this={dialog} out:dialogOut|global aria-label="Confirm byte copy" oncancel={event => { event.preventDefault(); void close(); }}>
  <header><h2>Copy to {request.side}</h2><span class="grow"></span><button class="icon" title={busy ? 'Cancel remaining copies' : 'Close'} onclick={() => void close()}><Icon name="close" /></button></header>
  {#if error}<p class="warn" role="alert">{error}</p>{/if}
  {#if busy}<p><span class="spin"></span> {preview ? 'Applying recoverable copies' : 'Preparing frozen preview'}</p>{/if}
  {#if preview}<p>{preview.files.length} files · {preview.files.filter(file => file.action === 'overwrite').length} replacements · {preview.retained} destination-only entries retained</p>
    <div class="operation-list">{#each preview.files as file}<div><span class="mono grow">{file.path}</span><span>{file.action}</span><span>{file.bytes.toLocaleString()} B</span></div>{/each}</div>
  {/if}
  {#if outcomes}<h3>Results</h3><div class="operation-list">{#each outcomes as outcome}<div><span class="mono grow">{outcome.path}</span><span class:warn={outcome.state !== 'applied'}>{outcome.record && undone.includes(outcome.record.id) ? 'undone' : outcome.state}</span></div>{#if outcome.error}<p class="warn">{outcome.error}</p>{/if}{/each}</div>{/if}
  <footer><button class="btn" title={app.capability('recovery').reason ?? 'Filesystem recovery'} disabled={busy || !app.capability('recovery').supported} onclick={() => app.recoveryOpen = true}><Icon name="refresh" /> Recovery</button><span class="grow"></span>
    {#if outcomes}<button class="btn" disabled={busy || !outcomes.some(outcome => outcome.record && !undone.includes(outcome.record.id))} onclick={undo}><Icon name="refresh" /> Undo batch</button>
    {:else}<button class="btn dark" disabled={busy || !preview || !!error} onclick={apply}><Icon name="copy" /> Confirm copy</button>{/if}
    <button class="btn" onclick={() => void close()}>{busy ? 'Cancel remaining' : 'Close'}</button></footer>
</dialog>