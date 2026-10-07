<script lang="ts">
  import { api, type HistoryCommit } from '../../../lib/api';
  import { formatCommitDate } from '../../../lib/history-graph';
  import type { RepoEntry } from '../../../lib/repositories';
  import { app } from '../../../lib/state.svelte';
  import SectionCard from './SectionCard.svelte';

  let { entry, count = 4, onmore }: { entry: RepoEntry; count?: number; onmore?: () => void } = $props();

  let commits = $state<HistoryCommit[] | null>(null);
  let failed = $state('');
  const path = $derived(app.dest(entry.item));

  $effect(() => {
    const target = path;
    let alive = true;
    api.repositoryHistory(target, count).then(history => { if (alive) commits = [...history.local, ...history.origin, ...(history.base ? [history.base] : []), ...history.below].slice(0, count); })
      .catch(reason => { if (alive) failed = String(reason); });
    return () => { alive = false; };
  });
</script>

<SectionCard title="History" sub="Newest commits">
  {#snippet actions()}{#if onmore}<button class="rf-link" onclick={onmore}>Open ›</button>{/if}{/snippet}
  {#if failed}<p class="warn" role="status">{failed}</p>
  {:else if !commits}<p class="mut"><span class="spin"></span> Reading history…</p>
  {:else if !commits.length}<p class="mut">No commits yet.</p>
  {:else}
    <ul class="rf-commits">
      {#each commits as commit (commit.sha)}
        <li><span class="history-sha">{commit.short}</span><span class="rf-commit-s" title={commit.subject}>{commit.subject}</span><span class="mut">{commit.author} · {formatCommitDate(commit.date)}</span></li>
      {/each}
    </ul>
  {/if}
</SectionCard>
