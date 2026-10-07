<script lang="ts">
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';
  import Popover from '../Popover.svelte';

  let { anchor, onclose }: { anchor: Element; onclose: () => void } = $props();

  const store = app.repositories;
  const current = $derived(store.inSetView ? app.ws.activeSet : null);

  function choose(id: string | null) {
    onclose();
    store.filter({ hostFilter: '', org: '', query: '' });
    app.openView(id ? { kind: 'set' } : { kind: 'repos' }, id ?? app.ws.activeSet);
  }
</script>

<Popover {anchor} label="Filter by set" {onclose} width={248}>
  <div class="menu-list" role="group" aria-label="Sets">
    <button class="menu-item" class:on={!current} aria-pressed={!current} onclick={() => choose(null)}><Icon name="repo" /><span class="lbl">Any set</span><span class="cnt">{store.everything.length}</span></button>
    {#each app.ws.sets as set (set.id)}
      <button class="menu-item" class:on={current === set.id} aria-pressed={current === set.id} onclick={() => choose(set.id)}><Icon name="folder" tone="folder" /><span class="lbl">{set.name}</span><span class="cnt">{set.items.length}</span></button>
    {/each}
    <hr />
    <button class="menu-item" onclick={() => { onclose(); app.newSet(); }}><Icon name="plus" /><span class="lbl">New set</span></button>
  </div>
</Popover>
