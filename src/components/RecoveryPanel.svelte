<script lang="ts">
  import { onMount } from 'svelte';
  import { confirm } from '../lib/confirm';
  import { api, type RecoveryRecord } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import Icon from './Icon.svelte';
  import { dialogOut } from '../lib/motion';
  let dialog: HTMLDialogElement;
  let records = $state<RecoveryRecord[]>([]), busy = $state(false), error = $state('');
  async function refresh() { records = (await api.recoveryList()).sort((left, right) => right.createdAt - left.createdAt); }
  async function run(record: RecoveryRecord, action: 'undo' | 'cleanup' | 'resolve') {
    if (busy) return;
    const message = action === 'cleanup' ? 'Permanently delete this recovery backup? This cannot be undone.'
      : action === 'resolve' ? 'Acknowledge this conflicting operation and allow new writes? Backups are retained, but automatic undo will remain disabled.'
      : 'Restore this operation? Later disk changes will not be overwritten.';
    if (!await confirm(message, { title: 'Filesystem recovery', kind: 'warning' })) return;
    busy = true; error = '';
    try {
      if (action === 'undo') {
        if (!await app.prepareDiskMutation()) return;
        await api.recoveryUndo(record.id);
      } else if (action === 'cleanup') await api.recoveryCleanup([record.id], true);
      else await api.recoveryResolve(record.id, true);
    } catch (reason) { error = String(reason); }
    finally { await refresh().catch(reason => { error = String(reason); }); busy = false; }
  }
  onMount(() => { dialog.showModal(); void refresh().catch(reason => { error = String(reason); }); });
</script>

<dialog class="operation-dialog" bind:this={dialog} out:dialogOut|global aria-label="Filesystem recovery" oncancel={event => { event.preventDefault(); if (!busy) app.recoveryOpen = false; }}>
  <header><h2>Filesystem recovery</h2><span class="grow"></span><button class="icon" title="Refresh recovery records" disabled={busy} onclick={() => refresh().catch(reason => error = String(reason))}><Icon name="refresh" /></button><button class="icon" title="Close recovery" disabled={busy} onclick={() => app.recoveryOpen = false}><Icon name="close" /></button></header>
  {#if error}<p class="warn" role="alert">{error}</p>{/if}
  <div class="operation-list">{#each records as record}<section>
    <div><strong class="mono grow">{record.path || record.id}</strong><span>{record.stage}</span><time>{new Date(record.createdAt).toLocaleString()}</time></div>
    <p class="mono faint">{record.root}</p>{#if record.warning}<p class="warn">{record.warning}</p>{/if}
    <div><button class="btn" disabled={busy || !['applied', 'replacing', 'prepared'].includes(record.stage)} onclick={() => run(record, 'undo')}><Icon name="refresh" /> Restore</button>
      <button class="btn" disabled={busy || record.stage !== 'replacing'} onclick={() => run(record, 'resolve')}>Acknowledge conflict</button>
      <span class="grow"></span><button class="btn" disabled={busy || !['undone', 'notApplied', 'conflict', 'incomplete'].includes(record.stage)} onclick={() => run(record, 'cleanup')}><Icon name="close" /> Delete backup</button></div>
  </section>{:else}<p>No recovery records.</p>{/each}</div>
</dialog>