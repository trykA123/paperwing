<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { app } from '../lib/state.svelte';
  import { confirmWith } from '../lib/confirm';
  import { dialogOut } from '../lib/motion';
  import { remoteConfirmMessage, SQUASH_NOTE, summarizeCleanup, type CleanupTarget } from '../lib/branch-cleanup';
  import { CleanupSession, type CleanupSide } from '../lib/branch-cleanup.svelte';
  import Icon from './Icon.svelte';
  import Alert from './Alert.svelte';
  import CleanupRepo from './cleanup/CleanupRepo.svelte';

  let { request }: { request: { targets: CleanupTarget[] } } = $props();
  const session = new CleanupSession(untrack(() => request.targets));
  const many = $derived(request.targets.length > 1);
  let dialog: HTMLDialogElement;
  let side = $state<CleanupSide>('local');

  const count = $derived(side === 'local' ? session.localCount : session.remoteCount);

  async function run() {
    if (side === 'remote') {
      const { accepted } = await confirmWith(remoteConfirmMessage(session.remotePlan()), { title: 'Delete remote branches', kind: 'warning', okLabel: 'Delete from remote', destructive: true });
      if (!accepted) return;
    }
    const results = await session.run(side);
    const { deleted, failed } = summarizeCleanup(results);
    app.toast(`${deleted} ${deleted === 1 ? 'branch' : 'branches'} deleted${failed ? `, ${failed} failed` : ''}`, failed ? 'warn' : 'success');
    if (results.length) void app.checkExists(request.targets.map(target => target.path));
  }

  function close() {
    if (!session.busy) app.cleanupDialog = null;
  }

  onMount(() => {
    dialog.showModal();
    void session.load();
    return () => session.dispose();
  });
</script>

<dialog class="operation-dialog cleanup-dialog" bind:this={dialog} out:dialogOut|global aria-label="Clean up merged branches"
  oncancel={event => { event.preventDefault(); close(); }}>
  <header>
    <h2>Clean up merged branches</h2>
    <span class="mut">{many ? `${request.targets.length} repositories` : request.targets[0]?.name}</span>
    <span class="grow"></span>
    <button class="icon" title="Close" aria-label="Close" disabled={session.busy} onclick={close}><Icon name="close" /></button>
  </header>

  <div class="seg cleanup-tabs" role="tablist" aria-label="Branches to clean up">
    {#each [['local', 'Local branches', session.localCount], ['remote', 'Remote branches', session.remoteCount]] as const as [id, label, picked] (id)}
      <button role="tab" aria-selected={side === id} class:on={side === id} disabled={session.busy} onclick={() => (side = id)}>{label}{#if picked}<small>{picked}</small>{/if}</button>
    {/each}
  </div>

  <div class="cleanup-body">
    <Alert kind="info">{SQUASH_NOTE}</Alert>
    {#if side === 'remote'}<Alert kind="warn">Remote deletion affects everyone who uses the remote. Nothing here is selected until you choose it, and you confirm once before anything is deleted.</Alert>{/if}
    {#each session.repos as repo (repo.target.path)}
      <CleanupRepo {repo} {session} {side} showName={many} />
    {/each}
  </div>

  <footer>
    <span class="hint">{side === 'local' ? 'Branches are never force-deleted; each must still be merged.' : 'One push per remote; main and master are never offered.'}</span>
    <span class="grow"></span>
    <button type="button" class="btn" disabled={session.busy} onclick={close}>Close</button>
    <button type="button" class="btn {side === 'remote' ? 'danger' : 'dark'}" disabled={!count || session.busy || session.loading} onclick={run}>
      {#if session.busy}<span class="spin"></span>{:else}<Icon name={side === 'remote' ? 'remote' : 'trash'} />{/if}
      {side === 'local' ? `Delete ${count} local ${count === 1 ? 'branch' : 'branches'}` : `Delete ${count} from remote…`}
    </button>
  </footer>
</dialog>
