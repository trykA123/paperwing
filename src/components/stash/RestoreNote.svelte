<script lang="ts">
  import type { RestoreState } from '../../lib/stash-switch';
  import { app } from '../../lib/state.svelte';
  import { plural } from '../../lib/plural';
  import Alert from '../Alert.svelte';

  let { state, path, ondrop }: { state: RestoreState; path: string; ondrop?: () => void } = $props();
</script>

{#if state.phase === 'applied'}
  <Alert kind="ok" role="status">
    Applied{state.indexRestored ? '' : ' (staged changes came back as unstaged)'}. {state.kept ? 'The stash is still saved.' : 'The stash is gone.'}
    {#snippet action()}{#if state.kept && ondrop}<button class="btn small" onclick={ondrop}>Drop stash…</button>{/if}{/snippet}
  </Alert>
{:else if state.phase === 'conflicted'}
  <Alert kind="err" role="alert" title="Conflicts. The stash is kept.">
    {plural(state.files.length, 'file')} conflict: <span class="mono">{state.files.join(', ')}</span>. Resolve them, then apply again or drop the stash.
    {#snippet action()}<button class="btn small" onclick={() => app.openVscode(path)}>Open repository</button>{/snippet}
  </Alert>
{:else if state.phase === 'failed'}
  <Alert kind="err" role="alert">{state.message}</Alert>
{:else if state.phase === 'dropped'}
  <Alert kind="info" role="status">Stash dropped.</Alert>
{/if}
