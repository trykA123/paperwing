<script lang="ts">
  import { untrack } from 'svelte';
  import { bulkTargets } from '../../lib/formation';
  import { confirmDeleteSet } from '../../lib/set-delete';
  import { needsClone, rowFacts } from '../../lib/row-actions';
  import { doingWord } from '../../lib/state/run-notices';
  import { headerPrimary } from '../../lib/header-primary';
  import { plural } from '../../lib/plural';
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';
  import SetEditPopover from './SetEditPopover.svelte';

  let editor = $state<HTMLButtonElement>();
  let editing = $state(false);
  const set = $derived(app.set);
  const temporary = $derived(app.temporary.find(set.id));
  const targets = $derived(bulkTargets(set.items, rowFacts));
  const toClone = $derived(set.items.filter(needsClone));
  const withChanges = $derived(targets.cloned.filter(item => (app.local[app.dest(item)]?.dirty ?? 0) > 0).length);
  const primary = $derived(headerPrimary({ running: app.running, itemCount: set.items.length, cloneCount: toClone.length, rightVisible: app.ws.shell.rightVisible, fetchable: targets.fetchable.length }));
  const progress = $derived(app.runProgress);
  const busy = $derived(app.running || app.gitBusy || app.clonePreparing);
  const missing = $derived(set.items.filter(item => app.refState(item) === 'missing').length);

  $effect(() => {
    app.ws.activeSet;
    editing = untrack(() => app.pendingRename);
    app.pendingRename = false;
  });
</script>

<section class="rf-setbar" class:temp={!!temporary} aria-label="Set {set.name}">
  <div class="rf-setinfo">
    <span class="rf-seticon"><Icon name="folder" size={16} tone="folder" /></span>
    <div>
      <b>{set.name}</b>{#if temporary}<span class="tag-temp">Temporary · not saved</span>{/if}
      <small>
        {#if temporary?.scanning}<span class="spin" aria-hidden="true"></span> Scanning, {set.items.length} found
        {:else}{plural(set.items.length, 'repository', 'repositories')} · {targets.cloned.length} cloned{#if toClone.length}{' · '}{toClone.length} to clone{/if}{#if withChanges}{' · '}<span class="rf-dirty">{withChanges} with changes</span>{/if}{#if missing}{' · '}<span class="warn">{missing} with a missing ref</span>{/if}{/if}
        {#if temporary}{' · '}<span class="mono" title={temporary.path}>{temporary.path}</span>{/if}
      </small>
    </div>
  </div>
  <div class="rf-setacts">
    {#if temporary}
      <button class="btn small dark" disabled={temporary.scanning} title={temporary.scanning ? 'Wait for the scan to finish' : 'Keep this set in the list of sets'} onclick={() => app.temporary.save(temporary.id)}><Icon name="check" /> Save as set</button>
      <button class="btn small" onclick={() => app.temporary.discard(temporary.id)}><Icon name="close" /> Discard</button>
    {:else}
      {#if app.running}
        <span class="btn small progress" class:dark={primary.progress} role="status" aria-label="{doingWord(app.runMode)} {progress.finished} of {progress.total}">
          <span class="bar" style:transform="scaleX({progress.pct / 100})"></span>
          <span class="lbl"><span class="spin"></span> {doingWord(app.runMode)} {progress.finished}/{progress.total}</span>
        </span>
      {:else}
        <button class="btn small" class:dark={primary.fetch} disabled={busy || !targets.fetchable.length} title={targets.fetchable.length ? `git fetch --prune in the ${plural(targets.fetchable.length, 'cloned repository', 'cloned repositories')}` : 'Nothing cloned to fetch'} onclick={() => app.startClone(targets.fetchable, 'fetch')}><Icon name="refresh" />Fetch {targets.fetchable.length}</button>
      {/if}
      <button class="btn small" disabled={busy || !targets.behind.length} title="Pull the repositories that are behind" onclick={() => app.startClone(targets.behind, 'pull')}><Icon name="download" tone="sync" />Pull {targets.behind.length}</button>
      <button class="btn small" disabled={busy || !targets.pushable.length} title="Push the repositories that are ahead" onclick={() => app.pushRepos(targets.pushable.map(item => ({ path: app.dest(item), name: app.folderOf(item) })))}><Icon name="upload" tone="sync" />Push {targets.pushable.length}</button>
      {#if toClone.length}<button class="btn small" class:dark={!app.ws.shell.rightVisible} disabled={busy} title="Clone the repositories that are not on disk" onclick={() => app.startClone(toClone)}><Icon name="folder" tone="sync" />Clone {toClone.length}</button>{/if}
      <button class="btn small" disabled={busy || !set.items.length || set.items.some(item => item.path)} onclick={() => app.openSetCompare()}><Icon name="copy" tone="inspect" />Compare set</button>
      <button class="btn small" bind:this={editor} aria-haspopup="dialog" aria-expanded={editing} onclick={() => (editing = !editing)}><Icon name="gear" />Edit</button>
      <button class="btn small icon-only" title="Delete set. Its repositories stay where they are." aria-label="Delete set" disabled={app.ws.sets.length < 2} onclick={() => confirmDeleteSet(set)}><Icon name="trash" /></button>
    {/if}
    <button class="btn small icon-only" title="Show every repository" aria-label="Leave this set" onclick={() => app.openView({ kind: 'repos' })}><Icon name="close" /></button>
  </div>
</section>

{#if editing && editor}<SetEditPopover anchor={editor} onclose={() => (editing = false)} />{/if}
