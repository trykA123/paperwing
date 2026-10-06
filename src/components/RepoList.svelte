<script lang="ts">
  import { untrack } from 'svelte';
  import { confirm } from '../lib/confirm';
  import { ago, app, matches } from '../lib/state.svelte';
  import { warningsForOrg } from '../lib/repo-notices';
  import type { Repo } from '../lib/api';
  import VirtualList from './VirtualList.svelte';
  import Pager from './Pager.svelte';
  import Icon from './Icon.svelte';
  import Alert from './Alert.svelte';
  import EmptyState from './EmptyState.svelte';
  import Skeleton from './Skeleton.svelte';
  import { countLabel, errorSummary } from '../lib/source-status';

  let { mode, source = '', org = '' }: { mode: 'org' | 'search'; source?: string; org?: string } = $props();

  const tab = untrack(() => app.activeTab);
  let filter = $state(tab?.filter ?? '');
  let page = $state(tab?.page ?? 0);
  $effect(() => { if (tab) { tab.filter = filter; tab.page = page; } });

  const src = $derived(app.sources.find(s => s.id === source));
  const base = $derived(mode === 'org' ? app.reposOf(source, org) : app.allRepos);
  const query = $derived(mode === 'org' ? filter : app.query);
  const list = $derived(query.trim() ? base.filter(r => matches(`${r.org}/${r.name} ${r.description}`, query)) : base);
  const size = $derived(app.ws.pageSize);
  const pages = $derived(size === 'all' ? 1 : Math.max(1, Math.ceil(list.length / size)));
  const cur = $derived(Math.min(page, pages - 1));
  const rows = $derived(size === 'all' ? list : list.slice(cur * size, cur * size + size));
  const loading = $derived(mode === 'org' ? !!app.loadingRepos[source] : Object.values(app.loadingRepos).some(Boolean));
  const errors = $derived(mode === 'org' ? (app.repoErrors[source] ?? []) : Object.values(app.repoErrors).flat());
  const unknown = $derived(!base.length && (loading || errors.length > 0));
  const failing = $derived(mode === 'org' ? (src ? [src] : []) : app.sources.filter(s => app.repoErrors[s.id]?.length));
  const warnings = $derived(mode === 'org' ? warningsForOrg(app.repoWarnings[source] ?? [], org) : Object.values(app.repoWarnings).flat());

  let previousQuery = untrack(() => query);
  $effect(() => {
    if (query !== previousQuery) page = 0;
    previousQuery = query;
  });

  const errorText = $derived(mode === 'org' ? errorSummary(errors) : failing.map(s => `${s.name}: ${errorSummary(app.repoErrors[s.id])}`).join(' \u00b7 '));
  const retry = () => failing.forEach(s => void app.loadRepos(s, true));

  async function addAll() {
    const toAdd = list.filter(r => !app.inSet(r.id));
    if (!toAdd.length) return app.toast('All shown repos are already in the set');
    if (toAdd.length > 25 && !(await confirm(`Add ${toAdd.length} repositories to "${app.set.name}"?`, { title: 'Add repos', okLabel: 'Add' }))) return;
    toAdd.forEach(r => app.addRepo(r, false));
    app.toast(`Added ${toAdd.length} repos to ${app.set.name}`, 'success');
  }
</script>

<header class="mh">
  <div class="grow">
    <div class="crumb">{mode === 'org' ? `Organization · ${src?.name ?? ''}` : 'Search'}</div>
    <h1>{mode === 'org' ? org : `“${app.query}”`}</h1>
    <div class="mut">
      {unknown ? countLabel(false, 0) : list.length}{query.trim() && mode === 'org' && !unknown ? ` of ${base.length}` : ''} repositories{mode === 'search' ? ' across all sources' : ''}
      {#if loading}· <span class="spin"></span> loading{/if}
    </div>
  </div>
  <div class="hbtns">
    {#if mode === 'org'}
      <label class="gsearch small"><Icon name="search" /><input placeholder="Filter {org}…" bind:value={filter} spellcheck="false" /></label>
    {/if}
    {#if src && src.kind !== 'manual'}
      <button class="btn" disabled={loading} onclick={() => app.loadRepos(src, true)} title="Fetch the repo list again from {src.host}">
        <Icon name="refresh" /> Refresh
      </button>
    {/if}
    <button class="btn" disabled={!list.length} onclick={addAll}><Icon name="plus" /> Add {list.length} shown</button>
    <button class="btn dark" onclick={() => app.openView({ kind: 'set' })}>Back to {app.set.name} →</button>
  </div>
</header>

{#if errors.length}
  <Alert kind="err" role="status" title="Can't reach {failing.map(s => s.name).join(', ') || 'the source'}">
    {errorText}
    {#snippet action()}<button class="btn small" disabled={loading} onclick={retry}>Retry</button>{/snippet}
  </Alert>
{/if}
{#if warnings.length}<Alert kind="warn" role="status" title={mode === 'org' ? 'Partial list' : 'Some repositories may be missing'}>{warnings.join(' · ')}</Alert>{/if}

<div class="card fill">
  {#key `${cur}|${size}|${query}`}
    <VirtualList items={rows} rowHeight={56} key={r => r.id}>
      {#snippet empty()}
        {#if loading}<Skeleton rows={8} height={56} />
        {:else if query.trim()}<EmptyState icon="search" title="No repositories match" hint="Try a different filter." />
        {:else}<EmptyState icon="folder" title="No repositories found" hint="Refresh the list, or check the source in Settings." />{/if}
      {/snippet}
      {#snippet row(r: Repo)}
        {@const n = app.countInSet(r.id)}
        <div class="rrow">
          <button class="star" class:on={app.ws.stars.includes(r.id)} title="Favorite" onclick={() => app.toggleStar(r.id)}>★</button>
          <div class="rn">
            <b>{r.name}{#if r.archived}<span class="badge">archived</span>{/if}</b>
            <small>{mode === 'search' ? `${r.org} · ` : ''}{r.description || 'No description'}</small>
          </div>
          <div class="meta">
            {#if r.defaultBranch}<span class="t-branch mono">{r.defaultBranch}</span>{/if}{#if r.pushedAt} · pushed {ago(r.pushedAt)}{/if}
          </div>
          <div class="addgrp">
            <button class="add" class:in={n > 0} onclick={() => app.toggleRepo(r)}>
              {n > 1 ? `✓ ${n}× in ${app.set.name}` : n ? `✓ In ${app.set.name}` : `+ Add to ${app.set.name}`}
            </button>
            {#if n}<button class="add in" title="Add another copy that clones into a different folder" onclick={() => app.addRepo(r)}>+1</button>{/if}
          </div>
        </div>
      {/snippet}
    </VirtualList>
  {/key}
  {#if list.length}<Pager total={list.length} bind:page bind:size={app.ws.pageSize} />{/if}
</div>
