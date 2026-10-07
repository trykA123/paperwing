<script lang="ts">
  import { untrack } from 'svelte';
  import { createCompareEditor, currentTheme, engineForText, LARGE_FILE_NOTICE, type CompareEditor } from '../lib/editor';
  import { languageId } from '../lib/languages';
  import { contentFromText } from '../lib/text-format';

  let { original, modified, path, inline = false }: { original: string; modified: string; path: string; inline?: boolean } = $props();
  let host: HTMLDivElement;
  let instance = $state.raw<CompareEditor | null>(null);
  let failed = $state('');
  const large = $derived(engineForText(original, modified) === 'viewer');

  $effect(() => {
    const left = contentFromText(original), right = contentFromText(modified), language = languageId(path), kind = engineForText(original, modified);
    const layout = untrack(() => (inline ? 'inline' : 'sideBySide'));
    let disposed = false, created: CompareEditor | undefined;
    createCompareEditor({ host, kind, left, right, settings: { layout, theme: currentTheme(), language, hideUnchanged: false, ignoreWhitespace: false, readOnly: { left: true, right: true }, locked: true } })
      .then(editor => { if (disposed) editor.dispose(); else { created = editor; instance = editor; } })
      .catch(reason => { failed = String(reason); });
    return () => { disposed = true; created?.dispose(); instance = null; };
  });

  $effect(() => { void instance?.configure({ layout: inline ? 'inline' : 'sideBySide' }); });
</script>

{#if failed}<p class="warn commit-empty">The compare view could not start: {failed}</p>{/if}
{#if large}<p class="editor-notice" role="status">{LARGE_FILE_NOTICE}</p>{/if}
<div class="diff-host editor-host" bind:this={host} hidden={!!failed}></div>
