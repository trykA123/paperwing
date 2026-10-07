<script lang="ts">
  import { app } from '../../lib/state.svelte';
  import Popover from '../Popover.svelte';

  let { anchor, onclose }: { anchor: Element; onclose: () => void } = $props();

  let name = $state(app.set.name);

  function save(event: Event) {
    event.preventDefault();
    const value = name.trim();
    if (value) app.set.name = value;
    onclose();
  }
</script>

<Popover {anchor} label="Edit set" {onclose} width={320}>
  <form class="set-edit" onsubmit={save}>
    <label class="fld"><span>Name</span><input bind:value={name} spellcheck="false" autocomplete="off" /></label>
    <div class="set-edit-foot"><button type="button" class="btn small" onclick={onclose}>Cancel</button><button class="btn small dark" disabled={!name.trim()}>Save</button></div>
  </form>
</Popover>
