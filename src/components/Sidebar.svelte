<script lang="ts">
  import { app } from '../lib/state.svelte';
  import type { Repo } from '../lib/api';
  import Icon from './Icon.svelte';

  const favs = $derived(app.ws.stars.map(id => app.repoById.get(id)).filter((r): r is Repo => !!r));

  function onSearch() {
    const query = app.query;
    app.openView(query.trim() ? { kind: 'search' } : { kind: 'set' }, app.ws.activeSet, query);
  }

  function openSet(id: string) {
    app.openView({ kind: 'set' }, id);
  }
</script>

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

{#if app.temporary.sets.length}
  <div class="sec">
    <h6>Opened from the file manager</h6>
    {#each app.temporary.sets as s (s.id)}
      <div class="set-nav">
        <button class="nav temp" class:on={s.id === app.ws.activeSet && (app.view.kind === 'set' || app.view.kind === 'item')} title="{s.path}" onclick={() => openSet(s.id)}>
          <Icon name="folder" tone="folder" /><span class="lbl">{s.name}</span>
          {#if s.scanning}<span class="spin" role="status" aria-label="Scanning for repositories"></span>{/if}
          <span class="tag-temp">Temporary</span><span class="cnt">{s.items.length}</span>
        </button>
        <button class="set-nav-x" aria-label="Discard {s.name}" title="Discard this temporary set" onclick={() => app.temporary.discard(s.id)}><Icon name="close" size={12} /></button>
      </div>
    {/each}
  </div>
{/if}

<div class="sec">
  <h6>Favorites</h6>
  {#each favs as r (r.id)}
    <button class="nav fav" title="Add to or remove from {app.set.name}" disabled={app.isTemporary} onclick={() => app.toggleRepo(r)}>
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
