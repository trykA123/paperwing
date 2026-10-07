<script lang="ts">
  import { app } from '../../lib/state.svelte';
  import Popover from '../Popover.svelte';
  import DestinationFields from '../right/DestinationFields.svelte';

  let { anchor, onclose }: { anchor: Element; onclose: () => void } = $props();
</script>

<Popover {anchor} label="Clone destination" {onclose} width={400} tall>
  <DestinationFields />
  <p class="mut sb-note">{app.ws.shallow ? 'Shallow' : 'Full history'} · {app.ws.parallel} parallel · <button class="link" onclick={() => { onclose(); app.openView({ kind: 'settings' }); }}>Change in Settings</button></p>
</Popover>
