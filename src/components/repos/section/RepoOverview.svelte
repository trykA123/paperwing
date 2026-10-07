<script lang="ts">
  import { describeRow } from '../../../lib/formation-row';
  import { plural } from '../../../lib/plural';
  import { repoCounts } from '../../../lib/repo-counts';
  import type { RepoSection } from '../../../lib/repo-sections';
  import type { RepoEntry } from '../../../lib/repositories';
  import { runNextAction } from '../../../lib/row-actions';
  import { app } from '../../../lib/state.svelte';
  import Icon from '../../Icon.svelte';
  import PullSection from '../../pulls/PullSection.svelte';
  import RepoActions from './RepoActions.svelte';
  import RepoChanges from './RepoChanges.svelte';
  import RepoMiniHistory from './RepoMiniHistory.svelte';
  import SectionCard from './SectionCard.svelte';

  let { entry, goto }: { entry: RepoEntry; goto: (section: RepoSection) => void } = $props();

  const counts = $derived(repoCounts(entry));
  const row = $derived(describeRow(entry.item, { focused: false, canAct: true }));
  const busy = $derived(app.running || app.gitBusy || app.clonePreparing);
  const tiles = $derived([
    { id: 'history' as const, label: 'Sync', value: row.sync.kind === 'rails' ? row.sync.label : 'Checking…', tone: row.sync.kind === 'rails' && row.sync.inSync ? 'ok' : '' },
    { id: 'changes' as const, label: 'Changes', value: counts.changes ? plural(counts.changes, 'file') : 'Clean', tone: counts.changes ? 'warn' : 'ok' },
    { id: 'prs' as const, label: 'Pull request', value: counts.prs === null ? '…' : counts.prs ? 'Open' : 'None', tone: '' },
    { id: 'stash' as const, label: 'Stash', value: counts.stash === null ? '…' : plural(counts.stash, 'entry', 'entries'), tone: '' },
  ]);
</script>

{#if entry.remoteOnly}
  <div class="rf-grid">
    <SectionCard title="Next step">
      <div class="rf-empty"><Icon name="cloud" size={18} /><b>Not cloned</b><p class="mut">Clone {entry.name} to see local changes, history and stashes.</p>
        <button class="btn dark" disabled={busy} onclick={() => runNextAction(entry.item, 'clone')}><Icon name="folder" />Clone</button></div>
    </SectionCard>
    <RepoActions {entry} />
  </div>
{:else}
  <div class="rf-tiles">
    {#each tiles as tile (tile.id)}
      <button class="rf-tile" onclick={() => goto(tile.id)}><small>{tile.label}</small><b class={tile.tone}>{tile.value}</b></button>
    {/each}
  </div>
  <div class="rf-grid">
    <div class="rf-col">
      {#if counts.changes}<RepoChanges {entry} limit={5} />{:else}<SectionCard title="Next step"><p class="mut rf-none">{row.next ? row.next.title : 'Nothing to do here. The working tree is clean and in sync.'}</p>{#if row.next}<button class="btn small dark" disabled={busy} onclick={() => runNextAction(entry.item, row.next!.kind)}>{row.next.label}</button>{/if}</SectionCard>{/if}
      <RepoMiniHistory {entry} onmore={() => goto('history')} />
    </div>
    <div class="rf-col">
      <SectionCard label="Pull request"><PullSection item={entry.item} drawer /></SectionCard>
      <RepoActions {entry} />
    </div>
  </div>
{/if}
