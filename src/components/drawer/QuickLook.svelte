<script lang="ts">
  import type { RepoChanges, SetItem } from '../../lib/api';
  import { describeRow } from '../../lib/formation-row';
  import { plural } from '../../lib/plural';
  import { runNextAction } from '../../lib/row-actions';
  import { detailsDrawer } from '../../lib/details-drawer.svelte';
  import { app } from '../../lib/state.svelte';
  import ChangedFiles from '../repos/ChangedFiles.svelte';
  import CommitList from '../repos/CommitList.svelte';
  import Icon from '../Icon.svelte';
  import PullSection from '../pulls/PullSection.svelte';
  import SyncRails from '../set/SyncRails.svelte';

  let { item, onready }: { item: SetItem; onready: () => void } = $props();

  const busy = $derived(app.running || app.gitBusy || app.clonePreparing);
  const row = $derived(describeRow(item, { focused: false, canAct: !busy }));
  const path = $derived(app.dest(item));
  const remote = $derived(row.remote || !row.local?.repo);
  const dirty = $derived(row.local?.dirty ?? 0);
  let changes = $state<RepoChanges | null>(null);

  const act = (run: () => void) => { run(); detailsDrawer.close(); };
  const open = () => act(() => app.repositories.openRepository(item.repoId));
  $effect(() => { onready(); });
</script>

<svelte:window onkeydown={event => { if (event.key === 'Enter' && !event.defaultPrevented && !(event.target as Element).closest('button, a, input')) { event.preventDefault(); open(); } }} />

<div class="history-body">
  <section class="ql-block" aria-label="Status">
    <p class="ql-status"><b>{remote ? 'Not cloned' : dirty ? plural(dirty, 'uncommitted file') : row.sync.kind === 'rails' ? row.sync.label : 'Checking…'}</b></p>
    {#if !remote}<SyncRails view={row.sync} />{/if}
    <div class="ql-actions">
      {#if row.next}{@const kind = row.next.kind}<button class="btn small dark" disabled={busy} title={row.next.title} aria-label={row.next.aria} onclick={() => act(() => runNextAction(item, kind))}>{row.next.label}</button>{/if}
      {#if !remote && !item.path}<button class="btn small" disabled={busy} onclick={() => act(() => void app.startClone([item], 'fetch'))}><Icon name="refresh" />Fetch</button>{/if}
    </div>
  </section>
  {#if remote}
    <p class="mut">Clone {item.name} to see local changes, history and pull requests here.</p>
  {:else}
    <section class="ql-block" aria-label="Changes">
      <h3>Changes <small>{changes ? plural(changes.files.length, 'file') : ''}</small></h3>
      <ChangedFiles {path} limit={3} bind:changes />
    </section>
    <section class="ql-block" aria-label="History">
      <h3>History</h3>
      <CommitList {path} name={app.folderOf(item)} count={4} />
    </section>
    <section class="ql-block" aria-label="Pull request"><PullSection {item} /></section>
  {/if}
</div>
<footer class="drawer-foot">
  <button class="btn dark" onclick={open}><Icon name="repo" />Open repository page</button>
  <span class="mut">Enter</span>
</footer>
