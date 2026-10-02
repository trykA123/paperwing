<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon, { type IconName } from './Icon.svelte';

  let { icon, title, hint, children }: { icon: IconName; title: string; hint?: string; children?: Snippet } = $props();
</script>

<div class="empty-state">
  <span class="empty-icon"><Icon name={icon} size={22} /></span>
  <h3>{title}</h3>
  {#if hint}<p>{hint}</p>{/if}
  {#if children}<div class="empty-actions">{@render children()}</div>{/if}
</div>
