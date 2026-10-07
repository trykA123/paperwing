<script lang="ts">
  import type { RailSection } from '../../lib/api';
  import { PAGES } from './registry';
  import PlaceholderPage from './PlaceholderPage.svelte';

  let { module }: { module: RailSection } = $props();

  const Page = $derived(PAGES[module] ?? PlaceholderPage);
</script>

<Page />
