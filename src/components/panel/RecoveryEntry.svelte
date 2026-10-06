<script lang="ts">
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';

  const capability = $derived(app.capability('recovery'));
</script>

<div class="sec panel-body">
  <h6>Filesystem recovery</h6>
  <p class="mut">Skein keeps a backup of every file it overwrites when you copy or save in a comparison. Open the list to restore or delete a backup.</p>
  {#if !capability.supported}<p class="warn" role="status">{capability.reason ?? 'Recovery is unavailable.'}</p>{/if}
  <button class="btn" disabled={!app.ready || !capability.supported || !!app.copyRequest} onclick={() => (app.recoveryOpen = true)}>
    <Icon name="undo" /> Open recovery
  </button>
</div>
