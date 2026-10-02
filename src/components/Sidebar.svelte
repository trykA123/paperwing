<script lang="ts">
  import { untrack } from 'svelte';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { app } from '../lib/state.svelte';
  import { motionMs } from '../lib/appearance';
  import type { Repo } from '../lib/api';
  import Icon from './Icon.svelte';

  const favs = $derived(app.ws.stars.map(id => app.repoById.get(id)).filter((r): r is Repo => !!r));
  let expanded = $state<string[]>([]);

  $effect(() => {
    const paths = app.set.items.filter(item => expanded.includes(`${app.set.id}:${item.id}`)).map(item => app.dest(item));
    untrack(() => { app.openTreePaths = paths; for (const path of paths) void app.loadTree(path); });
    return () => { app.openTreePaths = []; };
  });

  /** Git names stashes "On <branch>: <message>"; show the message and keep the branch as detail. */
  function stashParts(subject: string) {
    const match = subject.match(/^(?:WIP )?[Oo]n (.+?): ([\s\S]*)$/);
    return match ? { branch: match[1], message: match[2] } : { branch: '', message: subject };
  }

  function resize(event: PointerEvent) {
    const element = event.currentTarget as HTMLElement;
    element.setPointerCapture(event.pointerId);
    const move = (next: PointerEvent) => { app.ws.shell.sidebarWidth = Math.round(Math.max(190, Math.min(360, next.clientX))); };
    const finish = () => {
      element.removeEventListener('pointermove', move);
      element.removeEventListener('pointerup', finish);
      element.removeEventListener('pointercancel', finish);
    };
    element.addEventListener('pointermove', move);
    element.addEventListener('pointerup', finish);
    element.addEventListener('pointercancel', finish);
  }

  function onSearch() {
    const query = app.query;
    app.openView(query.trim() ? { kind: 'search' } : { kind: 'set' }, app.ws.activeSet, query);
  }

  function openSet(id: string) {
    app.openView({ kind: 'set' }, id);
  }
</script>

<aside class="side">
  <button class="side-resize" aria-label="Resize sidebar" title="Drag to resize sidebar; double-click to reset" onpointerdown={resize}
    ondblclick={() => (app.ws.shell.sidebarWidth = 250)} onkeydown={event => {
      if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
        event.preventDefault();
        app.ws.shell.sidebarWidth = Math.max(190, Math.min(360, app.ws.shell.sidebarWidth + (event.key === 'ArrowRight' ? 20 : -20)));
      }
    }}></button>

  <label class="gsearch">
    <Icon name="search" />
    <input placeholder="Search all repos…" bind:value={app.query} oninput={onSearch} spellcheck="false" />
  </label>

  <div class="sec">
    <h6>Sets</h6>
    {#each app.ws.sets as s (s.id)}
      <button class="nav" class:on={app.view.kind === 'set' && s.id === app.ws.activeSet} onclick={() => openSet(s.id)}>
        <Icon name="folder" tone="folder" /><span class="lbl">{s.name}</span><span class="cnt">{s.items.length}</span>
      </button>
    {/each}
    <button class="nav ghost" onclick={() => app.newSet()}>+ New set</button>
  </div>

  <div class="sec">
    <h6>{app.set.name} repositories</h6>
    {#each app.set.items as item (item.id)}
      {@const treeKey = `${app.set.id}:${item.id}`}
      {@const path = app.dest(item)}
      {@const tree = app.trees[path]}
      <div class="repo-nav" class:on={app.focusedItem?.id === item.id}>
        <button class="tree-toggle" class:expanded={expanded.includes(treeKey)} aria-expanded={expanded.includes(treeKey)} aria-label="Expand {app.folderOf(item)}"
          onclick={() => (expanded = expanded.includes(treeKey) ? expanded.filter(id => id !== treeKey) : [...expanded, treeKey])}><Icon name="disclosure" size={12} /></button>
        <button class="nav" title="Open {app.folderOf(item)} in {app.set.name}" onclick={() => app.openView({ kind: 'item', itemId: item.id })}>
          <Icon name={expanded.includes(treeKey) ? 'folderOpen' : 'folder'} tone="repo" /><span class="lbl">{app.folderOf(item)}</span>
          {#if app.local[app.dest(item)]?.dirty}<span class="cnt dirty" title="{app.local[app.dest(item)].dirty} changed or untracked file(s)">{app.local[app.dest(item)].dirty}</span>{/if}
        </button>
      </div>
      {#if expanded.includes(treeKey)}
        <div class="repo-subtree" transition:slide={{ duration: motionMs(180), easing: cubicOut }}>
          <button class="tree-refresh" title="Refresh repository tree" aria-label="Refresh {app.folderOf(item)} tree" disabled={tree?.loading} onclick={() => app.loadTree(path, true)}><Icon name="refresh" /></button>
          {#if tree?.loading}<span class="mut">Loading...</span>{/if}
          {#if tree?.error}<p class="tree-error" role="status">{tree.error}</p>{/if}
          {#if tree?.data}
            <details><summary><Icon name="disclosure" size={12} /><Icon name="branch" size={12} tone="branch" />Branches <small>{tree.data.branches.length}</small></summary>
              {#each tree.data.branches as branch}<div class="tree-entry" title={branch.sha}><Icon name="branch" size={12} tone="branch" /><span>{branch.name}</span>{#if branch.current}<Icon name="check" size={12} tone="ok" />{:else}<button class="tree-delete" title="Delete local branch (the remote is not touched)" aria-label="Delete local branch {branch.name}" disabled={app.running || app.gitBusy} onclick={() => app.deleteLocalBranch(path, app.folderOf(item), branch.name)}><Icon name="trash" size={12} /></button>{/if}</div>{:else}<span class="mut">None</span>{/each}
            </details>
            <details><summary><Icon name="disclosure" size={12} /><Icon name="remote" size={12} tone="repo" />Remotes <small>{tree.data.remotes.length}</small></summary>
              {#each tree.data.remotes as remote}<details class="remote-tree"><summary><Icon name="disclosure" size={12} /><Icon name="remote" size={12} tone="repo" />{remote.name}</summary>
                {#each remote.urls as url}<div class="tree-url">{url}</div>{/each}
                {#each remote.refs as reference}<div class="tree-entry" title={`${reference.sha} ${reference.symbolic}`}><Icon name="branch" size={12} tone="branch" /><span>{reference.name}</span></div>{/each}
              </details>{:else}<span class="mut">None</span>{/each}
            </details>
            <details><summary><Icon name="disclosure" size={12} /><Icon name="tag" size={12} tone="tag" />Tags <small>{tree.data.tags.length}</small></summary>
              {#each tree.data.tags as tag}<div class="tree-entry" title={tag.sha}><Icon name="tag" size={12} tone="tag" /><span>{tag.name}</span></div>{:else}<span class="mut">None</span>{/each}
            </details>
            <details><summary><Icon name="disclosure" size={12} /><Icon name="stash" size={12} tone="record" />Stashes <small>{tree.data.stashes.length}</small></summary>
              {#each tree.data.stashes as stash}{@const parts = stashParts(stash.subject)}<div class="tree-entry" title="{stash.name} · {stash.sha}"><Icon name="stash" size={12} tone="record" /><span>{parts.message}</span><small class="tree-meta">{stash.name}{parts.branch ? ` · ${parts.branch}` : ''}</small></div>{:else}<span class="mut">None</span>{/each}
            </details>
            <details><summary><Icon name="disclosure" size={12} /><Icon name="submodule" size={12} tone="folder" />Submodules <small>{tree.data.submodules.length}</small></summary>
              {#each tree.data.submodules as module}<div class="tree-entry" title={`${module.sha} ${module.url ?? ''}`}><Icon name="folder" size={12} tone="folder" /><span>{module.path}</span></div>{:else}<span class="mut">None</span>{/each}
            </details>
          {/if}
        </div>
      {/if}
    {:else}<div class="nav ghost">No repositories in this set</div>{/each}
  </div>

  <div class="sec">
    <h6>Favorites</h6>
    {#each favs as r (r.id)}
      <button class="nav fav" title="Add to or remove from {app.set.name}" onclick={() => app.toggleRepo(r)}>
        <span class="star">★</span><span class="lbl">{r.name} <small>{r.org}</small></span>
        {#if app.inSet(r.id)}<span class="inset">IN SET</span>{/if}
      </button>
    {:else}<div class="nav ghost">No favorites</div>{/each}
  </div>

  {#each app.sources as src (src.id)}
    <div class="sec">
      <h6>Browse · {src.name}{#if app.loadingRepos[src.id]}<span class="spin"></span>{/if}</h6>
      {#each app.orgsOf(src) as o (o)}
        {@const v = app.view}
        <button class="nav" class:on={v.kind === 'org' && v.source === src.id && v.org === o}
          onclick={() => app.openView({ kind: 'org', source: src.id, org: o })}>
          <span class="lbl">{o}</span><span class="cnt">{app.reposOf(src.id, o).length}</span>
        </button>
      {/each}
    </div>
  {/each}

  <div class="grow"></div>
  <button class="nav" class:on={app.view.kind === 'settings'} onclick={() => app.openView({ kind: 'settings' })}>
    <Icon name="gear" /> Settings
  </button>
</aside>
