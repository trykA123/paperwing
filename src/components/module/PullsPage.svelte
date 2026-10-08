<script lang="ts">
  import { untrack } from 'svelte';
  import { pullChip } from '../../lib/pull-chip';
  import { CHIP_QUEUES, pullStep } from '../../lib/pull-queue';
  import { pullQueue, type PullRow } from '../../lib/pull-queue.svelte';
  import { openPull, pulls } from '../../lib/pulls.svelte';
  import { plural } from '../../lib/plural';
  import { app } from '../../lib/state.svelte';
  import EmptyState from '../EmptyState.svelte';
  import Icon from '../Icon.svelte';
  import HostChip from './HostChip.svelte';
  import ScopeChip from './ScopeChip.svelte';
  import PullChip from '../pulls/PullChip.svelte';
  import ModuleTable from './ModuleTable.svelte';
  import PageFrame from './PageFrame.svelte';

  const counts = $derived(pullQueue.counts);
  const where = $derived(pullQueue.mode === 'all' ? 'all repositories' : app.set.name);
  const scope = $derived(app.modules.host ? `${where} on ${app.modules.host}` : where);
  const sub = $derived(`${plural(counts.open, 'open pull request')} in ${scope} · ${counts.review} need review${pulls.loading ? ' · loading…' : ''}`);

  $effect(() => {
    const keys = pullQueue.keys;
    if (pullQueue.autoLoad) void untrack(() => pulls.ensure(keys));
  });

  async function open(row: PullRow) {
    try { await openPull(row.pull.url); }
    catch { app.toast('Could not open the browser. Copy the link from the details panel.', 'error'); }
  }
</script>

<PageFrame crumb="Pull requests" title="Open pull requests" {sub}>
  {#snippet buttons()}
    <button class="btn" disabled={!pullQueue.keys.length} title="Ask GitHub again for every repository of the set" onclick={() => void pullQueue.refresh()}><Icon name="refresh" /> Refresh</button>
  {/snippet}
  {#snippet chips()}
    <div class="fm-filters" role="group" aria-label="Pull request queue">
      <ScopeChip module="prs" />
      <HostChip />
      {#each CHIP_QUEUES as chip (chip.id)}
        <button class="fm-chip" class:on={pullQueue.queue === chip.id} aria-pressed={pullQueue.queue === chip.id} onclick={() => pullQueue.select(chip.id)}>
          {#if chip.id !== 'open'}<i class="fm-dot {chip.id === 'review' ? 'behind' : chip.id === 'failing' ? 'ahead' : 'notCloned'}" aria-hidden="true"></i>{/if}{chip.label} <b>{counts[chip.id]}</b>
        </button>
      {/each}
    </div>
  {/snippet}
  <p class="sr-only" role="status" aria-live="polite">{pulls.limit ? pulls.limit.message : pulls.loading ? 'Loading pull requests' : ''}</p>
  <ModuleTable label="Pull requests in {where}" columns={['Pull request', 'Branch', 'Checks & review', 'Next action']} cols="minmax(200px, 1.6fr) minmax(150px, 1fr) minmax(190px, 1.3fr) minmax(120px, 164px)"
    items={pullQueue.rows} key={row => row.item.id} bind:page={pullQueue.page}>
    {#snippet cells(row: PullRow, index: number)}
      {@const step = pullStep(row.pull)}
      <div class="fm-row" role="row" aria-rowindex={index + 2}>
        <div class="fm-cell fm-repo" role="gridcell">
          <button class="fm-name" title="{row.pull.title} · open in the browser" onclick={() => open(row)}>{row.pull.title}</button>
          <small class="fm-sub"><span class="fm-org" title={row.pull.targetRepo}>#{row.pull.number} · {row.pull.targetRepo}</span></small>
        </div>
        <div class="fm-cell fm-branch" role="gridcell">
          <span class="fm-ref" title={row.key.branch}><span class="t-branch"><Icon name="branch" /></span><span class="nm">{row.key.branch}</span></span>
          <small class="fm-base">into {row.pull.base}</small>
        </div>
        <div class="fm-cell fm-pull" role="gridcell"><span class="pull-cell"><PullChip view={pullChip(row.pull)} onopen={() => open(row)} /></span></div>
        <div class="fm-cell fm-next" role="gridcell">
          <button class="btn small fm-action" title={step.title} onclick={() => open(row)}><Icon name={step.icon} tone="inspect" />{step.label}</button>
        </div>
      </div>
    {/snippet}
    {#snippet empty()}
      {#if pullQueue.needsClick && !pullQueue.known.length}
        <EmptyState icon="refresh" title="Pull requests are not loaded" hint="{plural(pullQueue.keys.length, 'repository', 'repositories')} in {pullQueue.mode === 'all' ? 'scope' : 'this set'}. Loading asks GitHub once for each.">
          <button class="btn dark" onclick={() => void pullQueue.load()}><Icon name="download" /> Load pull requests</button>
        </EmptyState>
      {:else if !pullQueue.keys.length}
        <EmptyState icon="folder" title="No repository on a branch" hint="Pull requests belong to the branch a cloned repository has checked out. Clone the set first." />
      {:else if pulls.loading}
        <EmptyState icon="refresh" title="Loading pull requests" hint="Asking GitHub for {plural(pullQueue.keys.length, 'repository', 'repositories')}." />
      {:else}
        <EmptyState icon="check" title="Nothing in this queue" hint="{pullQueue.mode === 'all' ? 'No repository' : `No repository of ${app.set.name}`} has a matching pull request on its current branch." />
      {/if}
    {/snippet}
  </ModuleTable>
</PageFrame>
