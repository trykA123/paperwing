<script lang="ts">
  import type { CleanupRepo, CleanupSession, CleanupSide } from '../../lib/branch-cleanup.svelte';
  import Alert from '../Alert.svelte';
  import Skeleton from '../Skeleton.svelte';
  import CleanupList from './CleanupList.svelte';

  let { repo, session, side, showName }: { repo: CleanupRepo; session: CleanupSession; side: CleanupSide; showName: boolean } = $props();
  const path = $derived(repo.target.path);
  const result = $derived(side === 'local' ? repo.result : repo.remoteResult);
</script>

<section class="cleanup-repo" aria-label={repo.target.name}>
  {#if showName}<h3>{repo.target.name}{#if repo.load.status === 'ready'}<small class="mut mono"> merged into {side === 'local' ? repo.load.data.base : repo.load.data.remoteBase ?? repo.load.data.base}</small>{/if}</h3>{/if}
  {#if repo.load.status === 'loading'}
    <Skeleton rows={2} height={28} />
  {:else if repo.load.status === 'error'}
    <Alert kind="err" role="alert">{repo.load.message}</Alert>
  {:else}
    {#if result}
      <Alert kind={result.error || result.failed.length ? 'err' : 'ok'} role="status">
        {#if result.error}{result.error}{:else}{result.deleted.length} deleted{result.failed.length ? `, ${result.failed.length} failed` : ''}{/if}
        {#each result.failed as item (item.name)}<br /><span class="mono">{item.name}</span>: {item.error ?? 'failed'}{/each}
      </Alert>
    {/if}
    {#if side === 'local'}
      <CleanupList label="Local branches" rows={repo.load.local} picked={repo.localPicked} disabled={session.busy}
        ontoggle={(name, on) => session.toggle(path, 'local', name, on)} onall={on => session.setAll(path, 'local', on)} />
    {:else if repo.load.data.remote}
      <CleanupList label="Branches on {repo.load.data.remote}" rows={repo.load.remote} picked={repo.remotePicked} disabled={session.busy}
        ontoggle={(name, on) => session.toggle(path, 'remote', name, on)} onall={on => session.setAll(path, 'remote', on)} />
    {:else}
      <p class="cleanup-empty mut">This repository has no remote.</p>
    {/if}
  {/if}
</section>
