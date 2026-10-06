<script lang="ts">
  import { untrack } from 'svelte';
  import { confirmWith } from '../../lib/confirm';
  import { bulkTargets } from '../../lib/formation';
  import { needsClone, rowFacts } from '../../lib/row-actions';
  import { doingWord } from '../../lib/state/run-notices';
  import { plural } from '../../lib/plural';
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';

  let editing = $state(false);
  const set = $derived(app.set);
  const temporary = $derived(app.temporary.find(set.id));
  const selected = $derived(app.selected);
  const missing = $derived(selected.filter(item => app.refState(item) === 'missing').length);
  const fetchable = $derived(bulkTargets(set.items, rowFacts).fetchable);
  const toClone = $derived(set.items.filter(needsClone).length);
  const progress = $derived(app.runProgress);
  const busy = $derived(app.running || app.gitBusy || app.clonePreparing);

  $effect(() => {
    app.ws.activeSet;
    editing = untrack(() => app.pendingRename);
    app.pendingRename = false;
  });

  const focus = (el: HTMLInputElement) => { el.focus(); el.select(); };

  function rename(event: Event) {
    const value = (event.currentTarget as HTMLInputElement).value.trim();
    if (value) set.name = value;
    editing = false;
  }

  async function deleteSet() {
    const hasFixed = set.items.some(item => item.path);
    const folders = set.items.filter(item => !item.path && app.exists[app.dest(item, set.id)]);
    const risky = folders.filter(item => { const l = app.local[app.dest(item, set.id)]; return !!l && (l.dirty > 0 || l.ahead > 0); }).length;
    const trash = app.capability('trash');
    if (app.nativePlatform === 'linux') {
      const result = await confirmWith(`Remove the set "${set.name}"? Folders stay on disk unless you choose recycling below. If any requested recycle fails, the set stays configured.`, {
        title: 'Remove set', kind: 'warning', okLabel: 'Remove configuration only', destructive: true,
        check: folders.length ? {
          label: `Also move the ${folders.length} cloned folder${folders.length === 1 ? '' : 's'} to desktop Trash`,
          okLabel: 'Recycle folders and remove set', disabled: !trash.supported || hasFixed,
          hint: hasFixed ? 'This set holds folders opened in place. Skein never removes those; use your file manager.'
            : !trash.supported ? trash.reason ?? 'Folder removal is unavailable.'
            : risky ? `${risky} folder(s) have uncommitted changes or unpushed commits that move with the folder.` : 'Shared, unsafe or changed folders stay in place.',
        } : undefined,
      });
      if (result.accepted) await app.deleteSet(set.id, result.checked && !hasFixed);
      return;
    }
    const result = await confirmWith(`Delete the set "${set.name}"? The repositories stay on disk unless you also remove them below.`, {
      title: 'Delete set', kind: 'warning', okLabel: 'Delete', destructive: true,
      check: folders.length ? {
        label: `Also move the ${folders.length} cloned folder${folders.length === 1 ? '' : 's'} to the Recycle Bin`,
        disabled: !trash.supported || hasFixed,
        hint: hasFixed ? 'This set holds folders opened in place. Skein never removes those; use your file manager.'
          : !trash.supported ? trash.reason ?? 'Folder removal is unavailable.'
          : risky ? `${risky} of them ${risky === 1 ? 'has' : 'have'} uncommitted changes or unpushed commits that would go with the folder.` : 'Folders used by another set, and anything that is not a Git repository, are left alone.',
      } : undefined,
    });
    if (result.accepted) await app.deleteSet(set.id, result.checked && !hasFixed);
  }
</script>

<header class="mh">
  <div class="grow">
    <div class="crumb">{temporary ? 'Temporary set · not saved' : 'Set'}</div>
    {#if editing}
      <input class="title-edit" value={set.name} use:focus onblur={rename}
        onkeydown={e => { if (e.key === 'Enter' || e.key === 'Escape') e.currentTarget.blur(); }} />
    {:else}
      <h1>{set.name}<button class="icon" title="Rename" aria-label="Rename set" onclick={() => (editing = true)}>✎</button></h1>
    {/if}
    <div class="mut">
      {#if temporary}<span class="mono" title={temporary.path}>{temporary.path}</span> · {/if}
      {#if temporary?.scanning}<span class="spin" aria-hidden="true"></span> Scanning, {set.items.length} found{:else}{#if selected.length}{selected.length} of {set.items.length} selected · <button class="link" onclick={() => app.setAllOn(false)}>Clear</button>{:else}{plural(set.items.length, 'repository', 'repositories')}{/if}{/if}{#if missing} · <span class="warn">{missing} with a missing ref</span>{/if}
    </div>
  </div>
  <div class="hbtns">
    {#if temporary}
      <button class="btn dark" disabled={temporary.scanning} title={temporary.scanning ? 'Wait for the scan to finish' : 'Keep this set in the list of sets'} onclick={() => app.temporary.save(temporary.id)}><Icon name="check" /> Save as set</button>
      <button class="btn" onclick={() => app.temporary.discard(temporary.id)}><Icon name="close" /> Discard</button>
    {:else}
      <button class="btn" disabled={!set.items.length} title="Search code across the repositories of this set (Ctrl+Shift+F)" onclick={() => app.openCodeSearch()}><Icon name="search" /> Search code</button>
      <button class="btn" onclick={() => app.goAddRepos()}><Icon name="plus" /> Add repositories</button>
      <button class="btn icon-only" title="Delete set" aria-label="Delete set" disabled={app.ws.sets.length < 2} onclick={deleteSet}><Icon name="trash" /></button>
      {#if app.running}
        <span class="btn dark progress" role="status" aria-label="{doingWord(app.runMode)} {progress.finished} of {progress.total}">
          <span class="bar" style:transform="scaleX({progress.pct / 100})"></span>
          <span class="lbl"><span class="spin"></span> {doingWord(app.runMode)} {progress.finished}/{progress.total}</span>
        </span>
      {:else}
        <button class="btn" class:dark={!toClone} disabled={busy || !fetchable.length} title={fetchable.length ? `git fetch --prune in the ${plural(fetchable.length, 'cloned repository', 'cloned repositories')}` : 'Nothing cloned to fetch'} onclick={() => app.startClone(fetchable, 'fetch')}><Icon name="refresh" /> Fetch all</button>
      {/if}
    {/if}
  </div>
</header>
