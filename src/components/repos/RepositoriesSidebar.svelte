<script lang="ts">
  import { errorSummary } from '../../lib/source-status';
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';
  import FavoriteNav from './FavoriteNav.svelte';

  const store = app.repositories;
  const clonedCount = $derived(store.everything.filter(entry => store.localOf(entry)?.repo).length);
  const changedCount = $derived(store.everything.filter(entry => (store.localOf(entry)?.dirty ?? 0) > 0).length);
  const failing = $derived(app.sources.filter(source => app.repoErrors[source.id]?.length));
  const loading = $derived(app.sources.filter(source => app.loadingRepos[source.id]));
  const onList = $derived(app.view.kind === 'repos' && !store.hostFilter && !store.org && !store.query);
  const hostOn = (host: string) => store.hostFilter === host && !store.org;

  function view(chip: 'all' | 'cloned' | 'changes') {
    store.filter({ chip, hostFilter: '', org: '', query: '' });
    app.openView({ kind: 'repos' });
  }

  function narrow(host: string, org = '') {
    const same = store.hostFilter === host && store.org === org;
    store.filter({ hostFilter: same ? '' : host, org: same ? '' : org });
    if (app.view.kind !== 'set') app.openView({ kind: 'repos' });
  }

  const openSet = (id: string) => app.openView({ kind: 'set' }, id);
</script>

<div class="sec">
  <h6>Views</h6>
  <button class="nav" class:on={onList && store.chip === 'all'} onclick={() => view('all')}><Icon name="repo" /><span class="lbl">All repositories</span><span class="cnt">{store.everything.length}</span></button>
  <button class="nav" class:on={onList && store.chip === 'cloned'} onclick={() => view('cloned')}><Icon name="check" tone="ok" /><span class="lbl">Cloned</span><span class="cnt">{clonedCount}</span></button>
  <button class="nav" class:on={onList && store.chip === 'changes'} onclick={() => view('changes')}><Icon name="changes" tone="record" /><span class="lbl">Has changes</span><span class="cnt">{changedCount}</span></button>
</div>

<FavoriteNav />

<div class="sec">
  <h6>Hosts and organizations{#if loading.length}<span class="spin" role="status" aria-label="Loading {loading.map(source => source.name).join(', ')}"></span>{/if}</h6>
  {#each failing as source (source.id)}
    <div class="src-error" role="status">
      <span class="src-error-text"><b>Can't reach {source.name}.</b> <span title={errorSummary(app.repoErrors[source.id])}>{errorSummary(app.repoErrors[source.id])}</span></span>
      <span class="src-error-actions">
        <button class="link" disabled={!!app.loadingRepos[source.id]} onclick={() => app.loadRepos(source, true)}>Retry</button>
        <button class="link" onclick={() => app.openView({ kind: 'settings' })}>Edit source</button>
      </span>
    </div>
  {/each}
  {#each store.tree as host (host.host)}
    <button class="nav rf-hostbtn" class:on={hostOn(host.host)} aria-pressed={hostOn(host.host)} title="Show only {host.host}" onclick={() => narrow(host.host)}>
      <Icon name="server" /><span class="lbl mono">{host.host}</span><span class="cnt">{host.count}</span>
    </button>
    {#each host.orgs as org (org.org)}
      <button class="nav p-sub" class:on={store.hostFilter === host.host && store.org === org.org} aria-pressed={store.hostFilter === host.host && store.org === org.org} onclick={() => narrow(host.host, org.org)}>
        <span class="lbl">{org.org}</span><span class="cnt">{org.count}</span>
      </button>
    {/each}
  {:else}
    {#if !loading.length && !failing.length}<div class="nav ghost">No sources yet</div>{/if}
  {/each}
</div>

<div class="sec">
  <h6>Sets<button class="rf-add-btn" aria-label="New set" title="New set" onclick={() => app.newSet()}><Icon name="plus" /></button></h6>
  {#each app.ws.sets as set (set.id)}
    <button class="nav" class:on={app.view.kind === 'set' && set.id === app.ws.activeSet} onclick={() => openSet(set.id)}>
      <Icon name="folder" tone="folder" /><span class="lbl">{set.name}</span><span class="cnt">{set.items.length}</span>
    </button>
  {/each}
</div>

{#if app.temporary.sets.length}
  <div class="sec">
    <h6>Opened from the file manager</h6>
    {#each app.temporary.sets as set (set.id)}
      <div class="set-nav">
        <button class="nav temp" class:on={set.id === app.ws.activeSet && (app.view.kind === 'set' || app.view.kind === 'item')} title={set.path} onclick={() => openSet(set.id)}>
          <Icon name="folder" tone="folder" /><span class="lbl">{set.name}</span>
          {#if set.scanning}<span class="spin" role="status" aria-label="Scanning for repositories"></span>{/if}
          <span class="tag-temp">Temporary</span><span class="cnt">{set.items.length}</span>
        </button>
        <button class="set-nav-x" aria-label="Discard {set.name}" title="Discard this temporary set" onclick={() => app.temporary.discard(set.id)}><Icon name="close" size={12} /></button>
      </div>
    {/each}
  </div>
{/if}
