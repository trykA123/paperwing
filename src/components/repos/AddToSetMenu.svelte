<script lang="ts">
  import type { SetItem } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';
  import Popover from '../Popover.svelte';

  let { items, anchor, onclose, clone = false }: { items: readonly SetItem[]; anchor: Element; onclose: () => void; clone?: boolean } = $props();

  const pick = (action: () => void) => { action(); onclose(); };
</script>

<Popover {anchor} label={clone ? 'Choose a set to clone into' : 'Add to set'} {onclose} width={248}>
  <div class="menu-list" role="group" aria-label="Sets">
    {#each app.ws.sets as set (set.id)}
      <button class="menu-item" onclick={() => pick(() => (clone ? void app.repositories.cloneInto(items, set) : app.repositories.addToSet(items, set)))}><Icon name="folder" tone="folder" /><span class="lbl">{set.name}</span><span class="cnt">{set.items.length}</span></button>
    {/each}
    <hr />
    <button class="menu-item" onclick={() => pick(() => app.repositories.newSet(items, clone))}><Icon name="plus" /><span class="lbl">New set from these</span></button>
  </div>
</Popover>
