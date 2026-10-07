<script lang="ts">
  import type { RailSection } from '../../lib/api';
  import { moduleById } from '../../lib/modules';
  import { PLACEHOLDER_COPY } from '../../lib/module-copy';

  let { module }: { module: RailSection } = $props();

  const def = $derived(moduleById(module));
  const copy = $derived(PLACEHOLDER_COPY[module] ?? { title: def.label, sub: '', note: 'This page is not built yet.' });
</script>

<header class="mh">
  <div class="grow">
    <div class="crumb">{def.label}</div>
    <h1>{copy.title}</h1>
    <div class="mut">{copy.sub}</div>
  </div>
</header>
<section class="card module-empty" aria-label="{def.label} page">
  <p>{copy.note}</p>
</section>
