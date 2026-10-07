<script lang="ts">
  import { untrack } from 'svelte';
  import type { SearchMatch, SetItem } from '../lib/api';
  import { isSearchLimitError, buildSearchRequest } from '../lib/search-request';
  import { matchLocation } from '../lib/search-results';
  import type { SearchSession } from '../lib/search.svelte';
  import { app } from '../lib/state.svelte';
  import Alert from './Alert.svelte';
  import SearchBar from './search/SearchBar.svelte';
  import SearchResults from './search/SearchResults.svelte';
  import SearchScope, { type Scope } from './search/SearchScope.svelte';

  let { session, setId }: { session: SearchSession; setId: string } = $props();
  const set = $derived(app.ws.sets.find(entry => entry.id === setId) ?? app.temporary.find(setId));
  const isCloned = (item: SetItem) => !!app.local[app.dest(item, setId)]?.repo;
  const cloned = $derived((set?.items ?? []).filter(isCloned));
  const selected = $derived(cloned.filter(item => item.on));
  const refs = $state<Record<string, string>>({});
  let scope = $state<Scope>(untrack(() => ((set?.items ?? []).some(item => item.on && isCloned(item)) ? 'selected' : 'set')));

  const chosen = $derived(scope === 'selected' && selected.length ? selected : cloned);
  const targets = $derived(chosen.map(item => ({ path: app.dest(item, setId), name: app.folderOf(item), gitRef: refs[app.dest(item, setId)] ?? '' })));
  const refsSet = $derived(targets.some(target => target.gitRef.trim()));
  const canSearch = $derived(!!session.form.pattern.trim() && targets.length > 0 && !session.active);

  const search = () => session.start(buildSearchRequest(targets, session.form), Object.fromEntries(targets.map(target => [target.path, target.name])));

  const retryAfterCancelAll = () => session.restartAfterCancellingAll(buildSearchRequest(targets, session.form), Object.fromEntries(targets.map(target => [target.path, target.name])));

  async function openMatch(repo: string, match: SearchMatch) {
    const path = matchLocation(repo, match);
    try { await navigator.clipboard.writeText(path); app.toast(`Skein has no single-file view yet. Copied ${path} (line ${match.line}).`, 'info'); }
    catch (reason) { app.toast(`Skein has no single-file view yet. Could not copy ${path}: ${reason}`, 'warn'); }
  }
</script>

<section class="code-search" aria-label="Code search">
  <header class="mh"><div class="grow"><div class="crumb">Search</div><h1>Code search</h1><div class="mut">{set?.name ?? 'Set'} · {cloned.length} cloned {cloned.length === 1 ? 'repository' : 'repositories'}</div></div></header>
  <SearchBar bind:form={session.form} perl={session.perl} active={session.active} {canSearch} {refsSet} onsearch={search} onstop={() => void session.cancel()} />
  <SearchScope bind:scope {selected} {cloned} skipped={(set?.items.length ?? 0) - cloned.length} {refs} {setId} disabled={session.active} />
  {#if session.error}
    <Alert kind="err" role="alert">{session.error}
      {#snippet action()}{#if isSearchLimitError(session.error ?? '')}<button class="btn small" onclick={retryAfterCancelAll}>Stop all searches and retry</button>{/if}{/snippet}
    </Alert>
  {/if}
  <SearchResults {session} onopen={openMatch} />
</section>
