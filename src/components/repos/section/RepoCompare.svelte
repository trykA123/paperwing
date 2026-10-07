<script lang="ts">
  import { commands } from '../../../lib/commands';
  import type { RepoEntry } from '../../../lib/repositories';
  import { app } from '../../../lib/state.svelte';
  import Icon from '../../Icon.svelte';
  import SectionCard from './SectionCard.svelte';

  let { entry }: { entry: RepoEntry } = $props();

  const compare = $derived(commands([entry.item]).find(command => command.id === 'compare'));
  const set = $derived(app.ws.sets.find(candidate => candidate.items.some(item => item.id === entry.item.id)));
  const reason = $derived(compare?.enabled ? undefined : compare?.reason ?? 'Not available right now');

  function compareSet() {
    if (!set) return;
    app.ws.activeSet = set.id;
    app.openSetCompare();
  }
</script>

<SectionCard title="Compare" sub="Opens the compare views">
  <ul class="rf-actions-list">
    <li><div><b>Branches, tags and commits</b><p class="mut">Compare two refs of this repository, read only.</p></div>
      <button class="btn small" disabled={!compare?.enabled} title={reason} onclick={() => app.openCompare(entry.item, true)}><Icon name="copy" tone="inspect" />Compare refs</button></li>
    <li><div><b>Working tree</b><p class="mut">Compare your uncommitted files with HEAD.</p></div>
      <button class="btn small" disabled={!compare?.enabled} title={reason} onclick={() => app.openCompare(entry.item, false)}><Icon name="copy" tone="inspect" />Compare working tree</button></li>
    {#if set && set.items.length > 1}
      <li><div><b>Every repository of {set.name}</b><p class="mut">See how this repository differs from the rest of its set.</p></div>
        <button class="btn small" disabled={app.running || set.items.some(item => item.path)} onclick={compareSet}><Icon name="copy" tone="inspect" />Compare set</button></li>
    {/if}
  </ul>
</SectionCard>
