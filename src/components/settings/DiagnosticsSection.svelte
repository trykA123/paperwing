<script lang="ts">
  import { formatByteSize, progressPercent, type DiagnosticsStore } from '../../lib/diagnostics.svelte';
  import Alert from '../Alert.svelte';
  import Icon from '../Icon.svelte';

  let { store }: { store: DiagnosticsStore } = $props();
  const percent = $derived(progressPercent(store.progress));

  $effect(() => {
    void store.refreshSamples();
    const timer = setInterval(() => void store.refreshSamples(), 5000);
    return () => clearInterval(timer);
  });
</script>

<section class="settings-section">
  <div class="section-head"><div class="grow"><h2>Diagnostics</h2><p class="mut">Anonymous performance data from this test build, saved as one file you choose to share.</p></div></div>

  <div class="card setting-list">
    <div class="setting-row">
      <span class="setting-label">
        <b>Recording performance data</b>
        <small>Collects timings, Git process counts, memory and CPU of Skein, WebView2 and git.exe, and rounded repository sizes. It never collects names of repositories, organizations, branches or files, paths, hosts or users.</small>
      </span>
      <span class="diag-samples" role="status"><span class="diag-dot" aria-hidden="true"></span>{store.samples.toLocaleString()} samples</span>
    </div>
    <div class="setting-row">
      <span class="setting-label"><b>Generate diagnostics</b><small>Reads repository sizes, then shows the exact file contents before anything is saved.</small></span>
      {#if store.phase === 'collecting'}
        <button class="btn" onclick={() => store.cancel()}>Cancel</button>
      {:else}
        <button class="btn dark" disabled={store.phase === 'saving'} onclick={() => store.generate()}><Icon name="refresh" /> Generate diagnostics</button>
      {/if}
    </div>
    {#if store.phase === 'collecting'}
      <div class="diag-progress" role="status">
        <span class="bar"><i style:width="{percent}%"></i></span>
        <span class="hint">{store.progress ? `Measuring repositories, ${store.progress.completed} of ${store.progress.total} steps` : 'Starting…'}</span>
      </div>
    {/if}
  </div>

  {#if store.error}<Alert kind="err" role="alert">{store.error}</Alert>{/if}

  {#if store.preview}
    <div class="card diag-preview">
      <div class="diag-preview-head">
        <b>Exact file contents</b>
        <span class="hint">{formatByteSize(store.bytes)}</span>
        <button class="btn dark" disabled={store.phase === 'saving'} onclick={() => store.save()}>Save file…</button>
      </div>
      <!-- svelte-ignore a11y_no_noninteractive_tabindex: a scrollable block must be reachable by keyboard -->
      <pre class="diag-json" tabindex="0" aria-label="Diagnostics file contents">{store.preview}</pre>
    </div>
  {/if}

  <p class="hint">Share the file privately, in chat or a private gist. Do not post it in the public repository.</p>
</section>
