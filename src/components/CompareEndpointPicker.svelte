<script lang="ts">
  import type { CompareEndpoint } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import CompareReferencePicker from './CompareReferencePicker.svelte';
  import Select, { type SelectOption } from './Select.svelte';
  let { endpoint = $bindable(), side, readOnly = false }: { endpoint: CompareEndpoint; side: string; readOnly?: boolean } = $props();
  const item = $derived(app.ws.sets.find(set => set.id === endpoint.setId)?.items.find(item => item.id === endpoint.itemId));
  const path = $derived(item ? app.dest(item, endpoint.setId) : '');
  const repos = $derived<SelectOption[]>(app.ws.sets.flatMap(set => set.items.map(repo => ({ value: JSON.stringify([set.id, repo.id]), label: app.folderOf(repo), group: set.name }))));
</script>

<div class="compare-endpoint">
  <span class="endpoint-side">{side}</span>
  <Select class="endpoint-repo" label="{side} repository" searchable searchPlaceholder="Find a repository" options={repos} value={JSON.stringify([endpoint.setId, endpoint.itemId])}
    onchange={next => { const [setId, itemId] = JSON.parse(next); endpoint = { ...endpoint, setId, itemId }; }} />
  <CompareReferencePicker bind:reference={endpoint.reference} paths={path ? [path] : []} label={side} {readOnly} />
  <span class="endpoint-access">{endpoint.reference.kind === 'workingTree' ? 'working tree' : 'read-only'}</span>
</div>
