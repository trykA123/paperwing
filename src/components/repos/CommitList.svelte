<script lang="ts">
  import { api, type HistoryCommit } from '../../lib/api';
  import { formatCommitDate } from '../../lib/history-graph';
  import { detailsDrawer } from '../../lib/details-drawer.svelte';
  import { layoutHistory } from '../../lib/history-graph';

  let { path, name, count = 4 }: { path: string; name: string; count?: number } = $props();

  let commits = $state<HistoryCommit[] | null>(null);
  let failed = $state('');

  $effect(() => {
    const target = path;
    let alive = true;
    api.repositoryHistory(target, count).then(history => { if (alive) commits = [...history.local, ...history.origin, ...(history.base ? [history.base] : []), ...history.below].slice(0, count); })
      .catch(reason => { if (alive) failed = String(reason); });
    return () => { alive = false; };
  });

  function show(commit: HistoryCommit, event: MouseEvent) {
    api.repositoryHistory(path, 50).then(history => {
      const row = layoutHistory(history).rows.find(entry => entry.commit?.sha === commit.sha);
      if (row) detailsDrawer.open({ kind: 'commit', path, name, row }, event.currentTarget as Element);
    }).catch(() => {});
  }
</script>

{#if failed}<p class="warn" role="status">{failed}</p>
{:else if !commits}<p class="mut"><span class="spin"></span> Reading history…</p>
{:else if !commits.length}<p class="mut">No commits yet.</p>
{:else}
  <ul class="rf-commits">
    {#each commits as commit (commit.sha)}
      <li><button class="rf-commit-b" title="Show this commit" onclick={event => show(commit, event)}><span class="history-sha">{commit.short}</span><span class="rf-commit-s">{commit.subject}</span><span class="mut">{commit.author} · {formatCommitDate(commit.date)}</span></button></li>
    {/each}
  </ul>
{/if}
