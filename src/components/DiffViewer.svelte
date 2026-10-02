<script lang="ts">
  import { onMount } from 'svelte';
  import type { editor as MonacoEditor } from 'monaco-editor';

  let { original, modified, path, inline = false }: { original: string; modified: string; path: string; inline?: boolean } = $props();
  let host: HTMLDivElement;
  let instance = $state<MonacoEditor.IStandaloneDiffEditor | null>(null);
  let monacoApi: typeof import('../lib/monaco') | null = null;
  let models: { original: MonacoEditor.ITextModel; modified: MonacoEditor.ITextModel } | null = null;
  let failed = $state('');

  onMount(() => {
    let disposed = false;
    let observer: MutationObserver | undefined;
    (async () => {
      const api = await import('../lib/monaco');
      if (disposed) return;
      monacoApi = api;
      const font = () => getComputedStyle(document.documentElement).getPropertyValue('--mono');
      const created = api.monaco.editor.createDiffEditor(host, {
        automaticLayout: true, readOnly: true, originalEditable: false, renderSideBySide: !inline, useInlineViewWhenSpaceIsLimited: false,
        renderOverviewRuler: true, diffAlgorithm: 'advanced', maxComputationTime: 0, minimap: { enabled: false },
        fontSize: 13, scrollBeyondLastLine: false, renderIndicators: true, theme: api.applyEditorTheme(), fontFamily: font(),
      });
      observer = new MutationObserver(() => { api.applyEditorTheme(); created.updateOptions({ fontFamily: font() }); });
      observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme', 'style'] });
      instance = created;
    })().catch(reason => { failed = String(reason); });
    return () => {
      disposed = true;
      observer?.disconnect();
      instance?.dispose();
      models?.original.dispose();
      models?.modified.dispose();
      instance = null;
      models = null;
    };
  });

  $effect(() => {
    if (!instance || !monacoApi) return;
    const { monaco, language } = monacoApi;
    const next = {
      original: monaco.editor.createModel(original, language(path)),
      modified: monaco.editor.createModel(modified, language(path)),
    };
    instance.setModel(next);
    const previous = models;
    models = next;
    previous?.original.dispose();
    previous?.modified.dispose();
  });

  $effect(() => { instance?.updateOptions({ renderSideBySide: !inline }); });
</script>

{#if failed}<p class="warn commit-empty">The compare view could not start: {failed}</p>{/if}
<div class="diff-host" bind:this={host} hidden={!!failed}></div>
