<script lang="ts">
  import type { RailSection } from '../../lib/api';
  import { moduleById } from '../../lib/modules';

  let { module }: { module: RailSection } = $props();
</script>

<div class="sec panel-body">
  <h6>{moduleById(module).label}</h6>
  <button class="nav on" aria-current="page"><span class="lbl">Overview</span></button>
</div>
