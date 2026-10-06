<script lang="ts">
  import { untrack } from 'svelte';
  import { confirmWith } from '../lib/confirm';
  import { app, DEFAULT_COLS, PHASE, RUNNING } from '../lib/state.svelte';
  import type { Ref, SetItem } from '../lib/api';
  import VirtualList from './VirtualList.svelte';
  import LocalCell from './set/LocalCell.svelte';
  import StatusCell from './set/StatusCell.svelte';
  import Pager from './Pager.svelte';
  import RefPicker from './RefPicker.svelte';
  import Icon from './Icon.svelte';
  import EmptyState from './EmptyState.svelte';
  import { commands, execute } from '../lib/commands';

  const tab = untrack(() => app.activeTab);
  let page = $state(tab?.page ?? 0);
  let editing = $state(false);
  let editingId = $state<string | null>(null);
  let checking = $state(false);
  let picker = $state<{ items: SetItem[]; anchor: DOMRect } | null>(null);
  let menu = $state<{ item: SetItem; x: number; y: number } | null>(null);
  $effect(() => { if (tab) tab.page = page; });

  const set = $derived(app.set);
  const items = $derived(set.items);
  const size = $derived(app.ws.pageSize);
  const pages = $derived(size === 'all' ? 1 : Math.max(1, Math.ceil(items.length / size)));
  const cur = $derived(Math.min(page, pages - 1));
  const rows = $derived(size === 'all' ? items : items.slice(cur * size, cur * size + size));
  const selected = $derived(app.selected);
  const allOn = $derived(items.length > 0 && items.every(i => i.on));
  const missing = $derived(selected.filter(i => app.refState(i) === 'missing').length);
  const cloned = $derived(selected.filter(i => app.local[app.dest(i)]?.repo));
  const behind = $derived(cloned.filter(i => (app.local[app.dest(i)]?.behind ?? 0) > 0));
  const offRef = $derived(cloned.filter(i => !app.onRef(i)));
  const pushable = $derived(cloned.filter(i => { const l = app.local[app.dest(i)]; return !!l?.branch && (l.ahead > 0 || !l.upstream); }));
  const gitBusy = $derived(app.running || app.gitBusy);

  $effect(() => {
    app.ws.activeSet;
    picker = null;
    editing = untrack(() => app.pendingRename);
    app.pendingRename = false;
  });

  // A freshly duplicated row opens straight into folder rename, on whichever page it landed.
  $effect(() => {
    const id = app.renameItemId;
    if (!id) return;
    app.renameItemId = null;
    const k = untrack(() => items.findIndex(i => i.id === id));
    if (k >= 0 && size !== 'all') page = Math.floor(k / size);
    editingId = id;
  });

  function saveFolder(item: SetItem, e: Event) {
    if (editingId !== item.id) return;
    app.renameFolder(item, (e.currentTarget as HTMLInputElement).value);
    editingId = null;
  }

  const focus = (el: HTMLInputElement) => { el.focus(); el.select(); };

  type ColKey = keyof typeof DEFAULT_COLS;
  const COLS: { key: ColKey; label: string }[] = [
    { key: 'repo', label: 'Repository' }, { key: 'checkout', label: 'Checkout' },
    { key: 'local', label: 'Local' }, { key: 'status', label: 'Status' },
  ];
  const cols = $derived(app.ws.cols);
  // Status soaks up any spare width; its stored value is its minimum.
  // Local and Status share the spare width (2:1); their stored widths are minimums, never below a readable floor.
  const ACTIONS_W = 120;
  const localMin = $derived(Math.max(cols.local, 280));
  const statusMin = $derived(Math.max(cols.status, 140));
  const colsCss = $derived(`40px ${cols.repo}px ${cols.checkout}px minmax(${localMin}px, 2fr) minmax(${statusMin}px, 1fr) ${ACTIONS_W}px`);
  const rowMin = $derived(40 + cols.repo + cols.checkout + localMin + statusMin + ACTIONS_W);

  function startCol(e: PointerEvent, key: ColKey) {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const x0 = e.clientX, w0 = app.ws.cols[key];
    const move = (ev: PointerEvent) => (app.ws.cols[key] = Math.round(Math.max(90, Math.min(700, w0 + ev.clientX - x0))));
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  }

  function openPicker(list: SetItem[], el: HTMLElement) {
    picker = { items: list, anchor: el.getBoundingClientRect() };
  }

  function pick(ref: Ref) {
    const targets = picker!.items;
    picker = null;
    if (targets.length === 1) return app.setRef(targets[0], ref);
    const ok = targets.filter(i => {
      const r = app.refs[i.url];
      return !!r && (ref.type === 'branch' ? r.branches : r.tags).includes(ref.name);
    });
    ok.forEach(i => app.setRef(i, ref));
    const miss = targets.filter(i => !ok.includes(i)).map(i => i.name);
    const tail = miss.length ? ` (not in ${miss.slice(0, 4).join(', ')}${miss.length > 4 ? ` +${miss.length - 4} more` : ''})` : '';
    app.toast(`${app.refLabel({ ...targets[0], ref })} applied to ${ok.length} of ${targets.length} repos${tail}`, miss.length ? 'warn' : 'success');
  }

  async function checkRefs() {
    checking = true;
    await app.ensureRefs(selected.map(i => i.url), true);
    checking = false;
  }

  async function deleteSet() {
    const folders = items.filter(item => app.exists[app.dest(item, set.id)]);
    const risky = folders.filter(item => { const l = app.local[app.dest(item, set.id)]; return !!l && (l.dirty > 0 || l.ahead > 0); }).length;
    const trash = app.capability('trash');
    if (app.nativePlatform === 'linux') {
      const result = await confirmWith(`Remove the set "${set.name}"? Folders stay on disk unless you choose recycling below. If any requested recycle fails, the set stays configured.`, {
        title: 'Remove set', kind: 'warning', okLabel: 'Remove configuration only', destructive: true,
        check: folders.length ? {
          label: `Also move the ${folders.length} cloned folder${folders.length === 1 ? '' : 's'} to desktop Trash`,
          okLabel: 'Recycle folders and remove set', disabled: !trash.supported,
          hint: !trash.supported ? trash.reason ?? 'Folder removal is unavailable.' : risky ? `${risky} folder(s) have uncommitted changes or unpushed commits that move with the folder.` : 'Shared, unsafe or changed folders stay in place.',
        } : undefined,
      });
      if (result.accepted) await app.deleteSet(set.id, result.checked);
      return;
    }
    const result = await confirmWith(`Delete the set "${set.name}"? The repositories stay on disk unless you also remove them below.`, {
      title: 'Delete set', kind: 'warning', okLabel: 'Delete', destructive: true,
      check: folders.length ? {
        label: `Also move the ${folders.length} cloned folder${folders.length === 1 ? '' : 's'} to the Recycle Bin`,
        disabled: !trash.supported,
        hint: !trash.supported ? trash.reason ?? 'Folder removal is unavailable.' : risky ? `${risky} of them ${risky === 1 ? 'has' : 'have'} uncommitted changes or unpushed commits that would go with the folder.` : 'Folders used by another set, and anything that is not a Git repository, are left alone.',
      } : undefined,
    });
    if (result.accepted) await app.deleteSet(set.id, result.checked);
  }

  function rename(e: Event) {
    const v = (e.currentTarget as HTMLInputElement).value.trim();
    if (v) set.name = v;
    editing = false;
  }

  function context(event: MouseEvent, item: SetItem) {
    event.preventDefault();
    menu = { item, x: Math.max(8, Math.min(event.clientX, innerWidth - 300)), y: Math.max(8, Math.min(event.clientY, innerHeight - 400)) };
  }

  function focusMenu(element: HTMLElement) { element.querySelector('button')?.focus(); }
</script>

<svelte:window onpointerdown={event => { if (!(event.target as Element).closest('.row-menu')) menu = null; }} />

<header class="mh">
  <div class="grow">
    <div class="crumb">Set</div>
    {#if editing}
      <input class="title-edit" value={set.name} use:focus onblur={rename}
        onkeydown={e => { if (e.key === 'Enter' || e.key === 'Escape') e.currentTarget.blur(); }} />
    {:else}
      <h1>{set.name}<button class="icon" title="Rename" onclick={() => (editing = true)}>✎</button></h1>
    {/if}
    <div class="mut">
      {items.length} repositories · {selected.length} selected{#if missing} · <span class="warn">{missing} with a missing ref</span>{/if}
    </div>
  </div>
  <div class="hbtns">
    <button class="btn" onclick={() => app.goAddRepos()}><Icon name="plus" /> Add repos</button>
    <button class="btn" disabled={app.running || !set.items.length} title={app.running ? 'A Git operation is already running' : set.items.length ? undefined : 'Add repositories to the set first'} onclick={() => app.openSetCompare()}><Icon name="copy" /> Compare across set</button>
    <button class="btn icon-only" title="Delete set" disabled={app.ws.sets.length < 2} onclick={deleteSet}><Icon name="trash" /></button>
  </div>
</header>

<div class="bulk">
    <b>{selected.length} selected</b>
    <button class="btn" disabled={!selected.length} title={selected.length ? undefined : 'Select at least one repository first'} onclick={e => openPicker(selected, e.currentTarget)}><Icon name="branch" tone="branch" /> Ref for selected</button>
    <button class="btn" disabled={!selected.length || checking} onclick={checkRefs} title={selected.length ? 'Ask each remote which branches and tags exist' : 'Select at least one repository first'}>
      {#if checking}<span class="spin"></span> Checking…{:else}<Icon name="check" tone="inspect" /> Check refs{/if}
    </button>
    <span class="hsep"></span>
    <button class="btn" disabled={app.running || !cloned.length} onclick={() => app.startClone(cloned, 'fetch')}
      title={app.running ? 'A Git operation is already running' : cloned.length ? `git fetch --prune for the ${cloned.length} selected repos that are already cloned` : 'None of the selected repositories is cloned yet'}><Icon name="refresh" tone="sync" /> Fetch</button>
    <button class="btn" disabled={app.running || !behind.length} onclick={() => app.startClone(behind, 'pull')}
      title={app.running ? 'A Git operation is already running' : behind.length ? 'Fast-forward the selected repos that are behind their upstream' : 'No selected repository is behind its upstream'}><Icon name="download" tone="sync" /> Pull{#if behind.length}<span class="nb">{behind.length}</span>{/if}</button>
    <button class="btn" disabled={gitBusy || !offRef.length} onclick={() => app.startClone(offRef, 'switch')}
      title={gitBusy ? 'A Git operation is already running' : offRef.length ? 'Fetch and check out the Checkout ref in every selected repo that is on something else' : 'Every selected repository is already on its checkout ref'}><Icon name="branch" tone="branch" /> Switch{#if offRef.length}<span class="nb">{offRef.length}</span>{/if}</button>
    <button class="btn" disabled={gitBusy || !pushable.length} onclick={() => app.pushRepos(pushable.map(item => ({ path: app.dest(item), name: app.folderOf(item) })))}
      title={gitBusy ? 'A Git operation is already running' : pushable.length ? 'Push the current branch of each selected repo that has unpushed commits or is not published yet' : 'Nothing to push in the selected repositories'}><Icon name="upload" tone="sync" /> Push{#if pushable.length}<span class="nb">{pushable.length}</span>{/if}</button>
    <button class="btn" disabled={gitBusy || !cloned.length} onclick={() => app.openBranchDialog(cloned)}
      title={gitBusy ? 'A Git operation is already running' : cloned.length > 1 ? `Create the same new branch in ${cloned.length} repositories` : cloned.length ? 'Create a new local branch in the selected repository' : 'Select cloned repositories first'}><Icon name="plus" tone="branch" /> New branch…</button>
</div>

<div class="card fill repository-table" class:running={app.running || app.clonePreparing} style:--cols={colsCss} style:--rowmin="{rowMin}px">
  {#key `${set.id}|${cur}|${size}`}
    <VirtualList items={rows} rowHeight={88} key={i => i.id}>
      {#snippet header()}
        <div class="srow shead">
          <div><input type="checkbox" checked={allOn} indeterminate={!allOn && items.some(item => item.on)} onchange={e => app.setAllOn(e.currentTarget.checked)} aria-label="Select all repositories in this set" /></div>
          {#each COLS as c (c.key)}
            <div class="hcell">{c.label}<button class="colgrip" aria-label="Resize {c.label} column" title="Drag to resize · double-click to reset"
              onpointerdown={e => startCol(e, c.key)} ondblclick={() => (app.ws.cols[c.key] = DEFAULT_COLS[c.key])}></button></div>
          {/each}
          <div></div>
        </div>
      {/snippet}
      {#snippet empty()}
        <EmptyState icon="folder" title="This set is empty" hint="Add repositories from an organization, the search, or your favorites.">
          <button class="btn dark" onclick={() => app.goAddRepos()}><Icon name="plus" /> Add repositories</button>
          <span class="mut">or press <kbd>Ctrl K</kbd> to search commands</span>
        </EmptyState>
      {/snippet}
      {#snippet row(item: SetItem)}
        {@const st = app.refState(item)}
        <div class="srow" role="group" aria-label="Repository {app.folderOf(item)}" class:off={!item.on} class:focused={app.focusedItem?.id === item.id} oncontextmenu={event => context(event, item)}>
          <div><input type="checkbox" checked={item.on} onchange={e => (item.on = e.currentTarget.checked)} aria-label="Select {app.folderOf(item)}" /></div>
          <div class="rn">
            {#if editingId === item.id}
              <input class="fname-edit" value={app.folderOf(item)} use:focus spellcheck="false" onblur={e => saveFolder(item, e)}
                onkeydown={e => { if (e.key === 'Enter') e.currentTarget.blur(); else if (e.key === 'Escape') editingId = null; }} />
            {:else}
              <button class="fname" class:clash={app.hasClash(item)} title="Folder name — click to rename" onclick={() => (editingId = item.id)}
                onkeydown={event => {
                  if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
                    event.preventDefault();
                    const bounds = event.currentTarget.getBoundingClientRect();
                    menu = { item, x: Math.max(8, Math.min(bounds.left, innerWidth - 300)), y: Math.max(8, Math.min(bounds.bottom, innerHeight - 400)) };
                  }
                }}>
                <b>{app.folderOf(item)}</b><span class="pen">✎</span>
              </button>
            {/if}
            <small class="sub" title="{item.org}/{item.name}">{#if item.folder}<span class="src">{item.name}</span>{/if}<span>{item.org}</span>{#if app.exists[app.dest(item)]}<span class="ondisk">on disk</span>{/if}</small>
          </div>
          <div>
            <button class="refpill" class:bad={st === 'missing'} onclick={e => openPicker([item], e.currentTarget)} title="Change branch, tag or commit">
              <span class="t-{item.ref.type}"><Icon name={item.ref.type} /></span><span class="nm">{app.refLabel(item)}</span><span class="car">▾</span>
            </button>
          </div>
          <LocalCell l={app.local[app.dest(item)]} on={app.onRef(item)} checkoutRef={app.refLabel(item)} running={app.running || app.clonePreparing} {gitBusy}
            actions={{ commit: () => app.openGitDialog('commit', item), switch: () => app.startClone([item], 'switch'), pull: () => app.startClone([item], 'pull'), push: () => app.pushRepos([{ path: app.dest(item), name: app.folderOf(item) }]) }} />
          <StatusCell j={app.jobs[item.id]} {st} stale={app.refStale(item)} refErr={app.refs[item.url]?.error} phaseLabel={app.jobs[item.id] ? PHASE[app.jobs[item.id].phase] : ''}
            jobRunning={!!app.jobs[item.id] && RUNNING.includes(app.jobs[item.id].phase)} clash={item.on && app.hasClash(item)} destination={app.dest(item)} refKind={item.ref.type} onopen={() => app.openVscode(app.dest(item))} />
          <div class="acts">
            <button class="x" title="Compare repository refs" onclick={() => app.openCompare(item)}><Icon name="code" tone="inspect" /></button>
            <button class="x" title="Open repository details" onclick={() => app.openView({ kind: 'item', itemId: item.id })}><Icon name="folder" tone="inspect" /></button>
            <button class="x" title="Clone this repo again into another folder" onclick={() => app.duplicateItem(item.id)}><Icon name="copy" /></button>
            <button class="x" title="Remove from set" onclick={() => app.removeItem(item.id)}>×</button>
          </div>
        </div>
      {/snippet}
    </VirtualList>
  {/key}
  {#if items.length}<Pager total={items.length} bind:page bind:size={app.ws.pageSize} />{/if}
</div>





{#if picker}
  <RefPicker items={picker.items} anchor={picker.anchor} onclose={() => (picker = null)} onpick={pick} />
{/if}

{#if menu}
  {@const item = menu.item}
  <div class="row-menu" role="menu" tabindex="-1" use:focusMenu style:left="{menu.x}px" style:top="{menu.y}px"
    onkeydown={event => { if (event.key === 'Escape') menu = null; }}>
    <button role="menuitem" onclick={() => { app.openView({ kind: 'item', itemId: item.id }); menu = null; }}><Icon name="folder" />Open repository details</button>
    <button role="menuitem" disabled={gitBusy || !app.local[app.dest(item)]?.repo} title="Read-only comparison of branches, tags or commits" onclick={() => { app.openCompare(item, true); menu = null; }}><Icon name="code" tone="inspect" />Compare</button>
    {#each commands([item]).filter(command => ['clone', 'fetch', 'pull', 'push', 'switch', 'commit', 'new-branch', 'code'].includes(command.id)) as command (command.id)}
      <button role="menuitem" title={command.reason ?? command.label} disabled={!command.enabled} onclick={() => { execute(command); menu = null; }}><Icon name={command.icon} tone={command.tone} />{command.label}</button>
    {/each}
    <button role="menuitem" onclick={() => { editingId = item.id; menu = null; }}>Rename folder</button>
    <button role="menuitem" onclick={() => { app.duplicateItem(item.id); menu = null; }}><Icon name="copy" />Duplicate into another folder</button>
    <button role="menuitem" onclick={() => { app.removeItem(item.id); menu = null; }}><Icon name="close" />Remove from set</button>
  </div>
{/if}
