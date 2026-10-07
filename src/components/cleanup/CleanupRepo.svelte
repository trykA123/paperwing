<script lang="ts">
  import type { CleanupRepo, CleanupSession } from '../../lib/branch-cleanup.svelte';
  import Alert from '../Alert.svelte';
  import Skeleton from '../Skeleton.svelte';
  import CleanupList from './CleanupList.svelte';

  let { repo, session, showName }: { repo: CleanupRepo; session: CleanupSession; showName: boolean } = $props();
  const path = $derived(repo.target.path);
</script>

<section class="cleanup-repo" aria-label={repo.target.name}>
  {#if showName}<h3>{repo.target.name}{#if repo.load.status === 'ready'}<small class="mut mono"> merged into {repo.load.data.base}</small>{/if}</h3>{/if}
  {#if repo.load.status === 'loading'}
    <Skeleton rows={2} height={28} />
  {:else if repo.load.status === 'error'}
    <Alert kind="err" role="alert">{repo.load.message}</Alert>
  {:else}
    {#if repo.result}
      <Alert kind={repo.result.error || repo.result.failed.length ? 'err' : 'ok'} role="status">
        {#if repo.result.error}{repo.result.error}{:else}{repo.result.deleted.length} deleted{repo.result.failed.length ? `, ${repo.result.failed.length} failed` : ''}{/if}
        {#each repo.result.failed as item (item.name)}<br /><span class="mono">{item.name}</span>: {item.error ?? 'failed'}{/each}
      </Alert>
    {/if}
    <CleanupList label="Local branches" rows={repo.load.local} picked={repo.localPicked} disabled={session.busy}
      ontoggle={(name, on) => session.toggle(path, name, on)} onall={on => session.setAll(path, on)} />
  {/if}
</section>
