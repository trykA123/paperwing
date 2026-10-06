<script lang="ts">
  import type { SetItem } from '../../lib/api';
  import { isCloned, nextAction, syncView } from '../../lib/formation';
  import { openHistory, rowFacts, runNextAction } from '../../lib/row-actions';
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';
  import SyncRails from '../set/SyncRails.svelte';
  import RepositoryTree from './RepositoryTree.svelte';

  let { item }: { item: SetItem } = $props();

  const local = $derived(app.local[app.dest(item)]);
  const cloned = $derived(isCloned(local));
  const next = $derived(nextAction(rowFacts(item)));
  const busy = $derived(app.running || app.gitBusy || app.clonePreparing);
  const branch = $derived(local?.branchLabel ?? local?.branch ?? local?.tagLabel ?? local?.tag ?? (cloned ? local?.sha : ''));
</script>

<div class="rsec item-details">
  <h3>{app.folderOf(item)}</h3>
  <div class="mut">{item.path ? item.path : `${item.org}/${item.name}`}</div>
  <dl>
    <dt>Folder</dt><dd class="mono path" title={app.dest(item)}>{app.dest(item)}</dd>
    <dt>Branch</dt><dd class="mono">{item.path ? branch || 'detached' : app.refLabel(item)}{#if !item.path && cloned && !app.onRef(item)} <span class="warn">· folder is on {branch}</span>{/if}</dd>
    <dt>Sync</dt><dd><SyncRails view={syncView(local)} /></dd>
    {#if cloned && local && local.upstream}<dt>Tracks</dt><dd class="mono">{local.upstreamLabel ?? local.upstream}</dd>{/if}
  </dl>
  <div class="item-actions">
    {#if next}<button class="btn small dark" disabled={busy} title={next.title} onclick={() => runNextAction(item, next.kind)}>{next.label}</button>{/if}
    {#if cloned}
      {#if next?.kind !== 'commit'}<button class="btn small" disabled={busy} onclick={() => app.openGitDialog('commit', item)}><Icon name="check" tone="record" /> Commit…</button>{/if}
      <button class="btn small" disabled={busy} onclick={() => app.openGitDialog('branch', item)}><Icon name="branch" tone="branch" /> New branch…</button>
      <button class="btn small" onclick={event => openHistory(item, event.currentTarget)}><Icon name="commit" tone="inspect" /> History</button>
      <button class="btn small" onclick={() => app.openVscode(app.dest(item))}><Icon name="code" /> VS Code</button>
    {/if}
  </div>
  {#if cloned}{#key item.id}<RepositoryTree {item} />{/key}{/if}
</div>
