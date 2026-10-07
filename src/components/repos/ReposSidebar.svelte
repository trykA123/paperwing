<script lang="ts">
  import { app } from '../../lib/state.svelte';
  import RepositoriesSidebar from './RepositoriesSidebar.svelte';
  import RepositorySidebar from './RepositorySidebar.svelte';
</script>

{#if app.view.kind === 'repo'}<RepositorySidebar view={app.view} />{:else}<RepositoriesSidebar />{/if}
