<script lang="ts">
  import type { RailSection } from '../../lib/api';
  import PlaceholderPage from './PlaceholderPage.svelte';

  let { module }: { module: RailSection } = $props();
</script>

<PlaceholderPage {module} />
