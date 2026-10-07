<script lang="ts">
  import type { Snippet } from 'svelte';

  let { crumb, title, sub, buttons, chips, children }: { crumb: string; title: string; sub: string; buttons?: Snippet; chips?: Snippet; children: Snippet } = $props();
</script>

<header class="mh">
  <div class="grow">
    <div class="crumb">{crumb}</div>
    <h1>{title}</h1>
    <div class="mut">{sub}</div>
  </div>
  {#if buttons}<div class="hbtns">{@render buttons()}</div>{/if}
</header>
{#if chips}{@render chips()}{/if}
{@render children()}
