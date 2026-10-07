<script lang="ts">
  import { needsClone } from '../../lib/row-actions';
  import { app } from '../../lib/state.svelte';
  import Popover from '../Popover.svelte';
  import ClonePlan from '../right/ClonePlan.svelte';
  import CloneFooter from '../right/CloneFooter.svelte';
  import DestinationFields from '../right/DestinationFields.svelte';

  let { anchor, onclose }: { anchor: Element; onclose: () => void } = $props();

  let name = $state(app.set.name);
  const toClone = $derived(app.set.items.filter(needsClone));

  function save(event: Event) {
    event.preventDefault();
    const value = name.trim();
    if (value) app.set.name = value;
    onclose();
  }
</script>

<Popover {anchor} label="Edit set" {onclose} width={400} tall>
  <form class="set-edit" onsubmit={save}>
    <label class="fld"><span>Name</span><input bind:value={name} spellcheck="false" autocomplete="off" /></label>
    <div class="set-edit-foot"><button type="button" class="btn small" onclick={onclose}>Cancel</button><button class="btn small dark" disabled={!name.trim()}>Save name</button></div>
  </form>
  <DestinationFields />
  <ClonePlan items={toClone} />
  {#if toClone.length}<CloneFooter items={toClone} />{/if}
</Popover>
