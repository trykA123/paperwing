<script lang="ts">
  import type { SearchMatch } from '../../lib/api';
  import type { SearchSession } from '../../lib/search.svelte';
  import { describeRepoStatus, SEARCH_ROW_HEIGHT, type SearchRow } from '../../lib/search-results';
  import Alert from '../Alert.svelte';
  import EmptyState from '../EmptyState.svelte';
  import Icon from '../Icon.svelte';
  import VirtualList from '../VirtualList.svelte';

  let { session, onopen }: { session: SearchSession; onopen: (repo: string, match: SearchMatch) => void } = $props();
  const summary = $derived(session.summary);
  const rowKey = (row: SearchRow) => row.key;
</script>

{#snippet row(item: SearchRow)}
  {#if item.kind === 'repo'}
    {@const state = describeRepoStatus(item.status)}
    <div class="cs-row cs-repo"><Icon name="folder" tone="repo" /><b>{item.name}</b><span class="mut">{item.count} {item.count === 1 ? 'match' : 'matches'}</span>
      <span class="cs-state {state.tone}">{state.label}</span>{#if item.status?.error}<span class="cs-error" title={item.status.error}>{item.status.error}</span>{/if}</div>
  {:else if item.kind === 'file'}
    <div class="cs-row cs-file"><Icon name="code" tone="file" /><span class="mono" title={item.path}>{item.path}</span><span class="mut">{item.count}</span></div>
  {:else if item.kind === 'context'}
    <div class="cs-row cs-context mono"><span class="cs-line-no">{item.line}</span><span class="cs-text">{item.text}</span></div>
  {:else}
    <button class="cs-row cs-match mono" title="Copy the path of this file" onclick={() => onopen(item.repo, item.match)}>
      <span class="cs-line-no">{item.match.line}:{item.match.column}</span><span class="cs-text">{item.match.text}</span></button>
  {/if}
{/snippet}

<div class="cs-results">
  <div class="cs-status" role="status" aria-live="polite">
    {#if session.status === 'starting' || session.status === 'running'}<span class="spin" aria-hidden="true"></span> Searching, {session.reposDone} of {session.reposTotal} repositories done, {session.matchCount} matches
    {:else if session.status === 'done' || session.status === 'cancelled'}{session.status === 'cancelled' ? 'Cancelled. ' : ''}{session.matchCount} matches in {session.reposTotal} repositories{#if summary?.failed}, {summary.failed} failed{/if}
    {/if}
  </div>
  {#if summary?.capped}<Alert kind="warn">The result limit was reached, so some matches are not shown. Narrow the pattern or the paths to see the rest.</Alert>{/if}
  {#if session.status === 'idle'}
    <EmptyState icon="search" title="Search every repository in the set" hint="Results appear here as each repository answers." />
  {:else if session.status !== 'failed'}
    <div class="cs-list">
      <VirtualList items={session.rows} rowHeight={SEARCH_ROW_HEIGHT} key={rowKey} {row}>
        {#snippet empty()}{#if session.status === 'done' || session.status === 'cancelled'}<EmptyState icon="search" title="No matches" hint="Nothing in the searched repositories matches this pattern." />{/if}{/snippet}
      </VirtualList>
    </div>
  {/if}
</div>
