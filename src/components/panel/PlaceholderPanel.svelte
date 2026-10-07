<script lang="ts">
  import { moduleById } from '../../lib/modules';
  import { app } from '../../lib/state.svelte';
</script>

<div class="sec panel-body">
  <h6>{moduleById(app.ws.shell.section).label}</h6>
  <button class="nav on" aria-current="page"><span class="lbl">Overview</span></button>
</div>
