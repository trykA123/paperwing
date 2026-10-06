<script lang="ts">
  import { formatCommitDate, type GraphRow } from '../lib/history-graph';

  let { row }: { row: GraphRow | null } = $props();
  const WHERE = { local: 'Local only', origin: 'Origin only', base: 'Shared base', below: 'Shared history', uncommitted: 'Working tree', more: 'Not loaded' } as const;
</script>

<section class="history-details" aria-label="Commit details">
  {#if row?.commit}
    <p class="history-subject">{row.commit.subject}</p>
    <dl>
      <dt>Commit</dt><dd class="mono">{row.commit.sha}</dd>
      <dt>Author</dt><dd>{row.commit.author}</dd>
      <dt>Date</dt><dd>{formatCommitDate(row.commit.date)}</dd>
      <dt>Where</dt><dd>{WHERE[row.kind]}</dd>
    </dl>
  {:else if row}
    <p class="history-subject">{row.label}</p>
    <p class="mut">{row.kind === 'uncommitted' ? 'These files have changes that are not committed yet.' : 'Load more commits to see the rest of this rail.'}</p>
  {:else}
    <p class="mut">Hover or focus a commit to see its details.</p>
  {/if}
</section>
