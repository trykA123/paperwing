<script lang="ts">
  import { pullQueue } from '../../lib/pull-queue.svelte';
  import { scopeLabel, scopeOf, toggleScope, type ScopedModule } from '../../lib/scope';
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';

  let { module }: { module: ScopedModule } = $props();

  const mode = $derived(scopeOf(app.ws.shell, module));
  const label = $derived(scopeLabel(mode, app.set.name));

  function toggle() {
    toggleScope(app.ws.shell, module);
    if (module === 'prs') pullQueue.page = 0;
  }
</script>

<button class="fm-chip scope-chip" class:on={mode === 'all'} aria-pressed={mode === 'all'} title={mode === 'set' ? 'Show every repository, not only this set' : 'Show only this set'} onclick={toggle}>
  <Icon name="folder" />{label}<span class="car" aria-hidden="true">⇄</span>
</button>
