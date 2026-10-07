<script lang="ts">
  import { bulkTargets, filterCounts, isCloned, isDiverged } from '../../lib/formation';
  import { pushTarget, rowFacts } from '../../lib/row-actions';
  import { ago, app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';

  const set = $derived(app.set);
  const locals = $derived(set.items.map(item => app.local[app.dest(item)]));
  const counts = $derived(filterCounts(locals));
  const diverged = $derived(locals.filter(isDiverged).length);
  const inSync = $derived(locals.filter(local => isCloned(local) && !local!.dirty && !local!.ahead && !local!.behind && !!local!.upstream).length);
  const targets = $derived(bulkTargets(set.items, rowFacts));
  const busy = $derived(app.running || app.gitBusy || app.clonePreparing);
  const fetchedAt = $derived(app.lastFetch[set.id]);
  const rows = $derived([
    { label: 'In sync', n: inSync }, { label: 'Uncommitted changes', n: counts.changes }, { label: 'Behind', n: counts.behind - diverged },
    { label: 'Ahead', n: counts.ahead - diverged }, { label: 'Diverged', n: diverged }, { label: 'Not cloned', n: counts.notCloned },
  ]);
</script>

<div class="rsec set-summary">
  <h3>{set.name}</h3>
  <div class="mut">{set.items.length} repositories{#if fetchedAt}{" "}· fetched {ago(new Date(fetchedAt).toISOString())}{:else}{" "}· not fetched this session{/if}</div>
  <dl>
    {#each rows as row (row.label)}<dt>{row.label}</dt><dd class:zero={!row.n}>{row.n}</dd>{/each}
  </dl>
  <div class="item-actions">
    <button class="btn small" disabled={busy || !targets.fetchable.length} onclick={() => app.startClone(targets.fetchable, 'fetch')}><Icon name="refresh" /> Fetch all</button>
    <button class="btn small" disabled={busy || !targets.behind.length} onclick={() => app.startClone(targets.behind, 'pull')}><Icon name="download" tone="sync" /> Pull {targets.behind.length || ''}</button>
    <button class="btn small" disabled={busy || !targets.pushable.length} onclick={() => app.pushRepos(targets.pushable.map(pushTarget))}><Icon name="upload" tone="sync" /> Push {targets.pushable.length || ''}</button>
  </div>
  <p class="mut">Select a repository to see its details and history.</p>
</div>
