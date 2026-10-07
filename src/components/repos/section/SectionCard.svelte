<script lang="ts">
  import type { Snippet } from 'svelte';

  let { title = '', label = title, sub = '', actions, flush = false, children }: { title?: string; label?: string; sub?: string; actions?: Snippet; flush?: boolean; children: Snippet } = $props();
</script>

<section class="rf-card" aria-label={label}>
  {#if title}<div class="rf-card-h"><h2>{title}</h2>{#if sub}<small>{sub}</small>{/if}<span class="grow"></span>{#if actions}{@render actions()}{/if}</div>{/if}
  <div class="rf-card-b" class:flush class:embed={!title}>{@render children()}</div>
</section>
