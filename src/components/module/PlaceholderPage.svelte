<script lang="ts">
  import { moduleById } from '../../lib/modules';
  import { PLACEHOLDER_COPY } from '../../lib/module-copy';
  import { app } from '../../lib/state.svelte';
  import HostChip from './HostChip.svelte';
  import PageFrame from './PageFrame.svelte';

  const module = $derived(app.view.kind === 'module' ? app.view.module : 'changes');
  const def = $derived(moduleById(module));
  const copy = $derived(PLACEHOLDER_COPY[module] ?? { title: def.label, sub: '', note: 'This page is not built yet.' });
</script>

<PageFrame crumb={def.label} title={copy.title} sub={app.modules.host ? `${copy.sub} · ${app.modules.host}` : copy.sub}>
  {#snippet chips()}{#if app.modules.host}<div class="fm-filters"><HostChip /></div>{/if}{/snippet}
  <section class="card module-empty" aria-label="{def.label} page">
    <p>{copy.note}</p>
  </section>
</PageFrame>
