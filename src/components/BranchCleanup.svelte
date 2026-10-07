<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { app } from '../lib/state.svelte';
    import { dialogOut } from '../lib/motion';
  import { SQUASH_NOTE, summarizeCleanup, type CleanupTarget } from '../lib/branch-cleanup';
  import { CleanupSession } from '../lib/branch-cleanup.svelte';
  import Icon from './Icon.svelte';
  import Alert from './Alert.svelte';
  import CleanupRepo from './cleanup/CleanupRepo.svelte';

  let { request }: { request: { targets: CleanupTarget[] } } = $props();
  const session = new CleanupSession(untrack(() => request.targets));
  const many = $derived(request.targets.length > 1);
  let dialog: HTMLDialogElement;
  let confirming = $state(false);

  const count = $derived(session.localCount);

  async function run() {
    if (confirming || session.busy) return;
    confirming = true;
    try {
      const results = await session.run();
      if (!results.length) return;
      const { deleted, failed } = summarizeCleanup(results);
      app.toast(`${deleted} ${deleted === 1 ? 'branch' : 'branches'} deleted${failed ? `, ${failed} failed` : ''}`, failed ? 'warn' : 'success');
      void app.checkExists(request.targets.map(target => target.path));
    } finally { confirming = false; }
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

  <div class="cleanup-body">
    <Alert kind="info">{SQUASH_NOTE}</Alert>
    {#each session.repos as repo (repo.target.path)}
      <CleanupRepo {repo} {session} showName={many} />
    {/each}
  </div>

  <footer>
    <span class="hint">Branches are never force-deleted; each must still be merged.</span>
    <span class="grow"></span>
    <button type="button" class="btn" disabled={session.busy} onclick={close}>Close</button>
    <button type="button" class="btn dark" disabled={!count || session.busy || session.loading || confirming} onclick={run}>
      {#if session.busy}<span class="spin"></span>{:else}<Icon name="trash" />{/if}
      Delete {count} local {count === 1 ? 'branch' : 'branches'}
    </button>
  </footer>
</dialog>
