<script lang="ts">
  import { api, type CompareContent } from '../lib/api';
  import type { CompareState } from '../lib/compare.svelte';
  import type { View } from '../lib/workspace';
  import { refLabel } from '../lib/compare-view';
  let { view, comparison }: { view: Extract<View, { kind: 'fileDiff' }>; comparison: CompareState } = $props();
  const snapshot = $derived(comparison.snapshot);
  const stale = $derived(!snapshot || snapshot.id !== view.sessionId || snapshot.generation !== view.generation);
  const file = $derived(comparison.files.find(file => file.id === view.fileId));
  let contents = $state<(CompareContent | null)[]>([]);
  let error = $state('');
  let loading = $state(false);
  $effect(() => {
    let active = true;
    if (stale || !file) { contents = []; return; }
    loading = true; error = '';
    Promise.all((['left', 'right'] as const).map(side => file[side] ? api.comparisonContent(view.sessionId, view.generation, view.fileId, side) : null))
      .then(result => { if (active) contents = result; })
      .catch(reason => { if (active) error = typeof reason === 'object' && reason?.message ? reason.message : String(reason); })
      .finally(() => { if (active) loading = false; });
    return () => { active = false; };
  });
  function text(content: CompareContent | null) {
    if (!content) return 'Not present on this side.';
    if (content.binary || content.kind === 'gitlink') return 'Binary or repository entry; text preview is not available.';
    try { return new TextDecoder('utf-8', { fatal: true }).decode(new Uint8Array(content.bytes)); }
    catch { return 'This encoding is not supported by the text preview.'; }
  }
</script>
<section class="file-preview"><header class="compare-summary"><strong class="mono">{view.path}</strong><span class="grow"></span><span class="faint">Read-only</span></header>
  {#if stale}<p class="compare-message">This comparison has changed. Reopen the file from its folder comparison.</p>
  {:else if loading}<p class="compare-message"><span class="spin"></span> Loading...</p>
  {:else if error}<p class="compare-message warn">{error}</p>
  {:else if snapshot}<div class="file-preview-panes">{#each contents as content, index}<section><header>{index === 0 ? 'Left' : 'Right'} @ {refLabel(index === 0 ? snapshot.left.endpoint.reference : snapshot.right.endpoint.reference)}</header><pre>{text(content)}</pre></section>{/each}</div>{/if}
</section>