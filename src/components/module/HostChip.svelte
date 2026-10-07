<script lang="ts">
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';
</script>

{#if app.modules.host}
  <button class="fm-chip host-chip" aria-label="Showing {app.modules.host}. Show every host" title="Show every host" onclick={() => (app.modules.host = null)}>
    <Icon name="server" />{app.modules.host}<span class="car" aria-hidden="true">✕</span>
  </button>
{/if}
