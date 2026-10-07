<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { app, DEFAULT_TEMPLATE, PATH_TOKENS } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';

  let tplInput = $state<HTMLInputElement>();
  let contextBusy = $state(false);
  async function changeContext(change: () => void) {
    if (contextBusy || app.running || app.clonePreparing) return;
    contextBusy = true;
    try { if (await app.guardBuffers()) change(); }
    finally { contextBusy = false; }
  }
  async function changeInput(event: Event, key: 'root' | 'pathTemplate') {
    const input = event.currentTarget as HTMLInputElement;
    const value = input.value;
    if (key === 'root') {
      if (contextBusy || app.running || app.clonePreparing) return;
      contextBusy = true;
      try { await app.chooseRoot(value); } finally { contextBusy = false; }
    } else await changeContext(() => { app.ws[key] = value; });
    input.value = app.ws[key];
  }

  async function insertToken(token: string) {
    const t = app.ws.pathTemplate;
    const at = tplInput?.selectionStart ?? t.length;
    const sep = at > 0 && !/[\\/]$/.test(t.slice(0, at)) ? '\\' : '';
    await changeContext(() => { app.ws.pathTemplate = t.slice(0, at) + sep + token + t.slice(at); });
    tplInput?.focus();
  }
  async function browse() {
    const dir = await open({ directory: true, defaultPath: app.rootSupport.valid ? app.ws.root : undefined, title: 'Choose where repos are cloned' });
    if (typeof dir === 'string') {
      contextBusy = true;
      try { await app.chooseRoot(dir); } finally { contextBusy = false; }
    }
  }
</script>

<div class="rsec">
    <h3>Destination</h3>
    <div class="rootrow">
      <input class="big" aria-label="Destination root" aria-invalid={!app.rootSupport.valid} aria-describedby="root-support" placeholder="Choose a native destination folder" value={app.ws.root} onchange={event => changeInput(event, 'root')} disabled={!app.ready || contextBusy || app.running || app.clonePreparing} spellcheck="false" />
      <button class="btn icon-only" title="Choose folder" disabled={!app.ready || contextBusy || app.running || app.clonePreparing} onclick={browse}><Icon name="folder" tone="folder" /></button>
      <button class="btn icon-only" title="Open {app.ws.root} in VS Code" disabled={!app.rootSupport.valid} onclick={() => app.openVscode(app.ws.root)}><Icon name="code" /></button>
    </div>
    {#if !app.rootSupport.valid}<p id="root-support" class="warn" role="status">{app.rootSupport.reason ?? 'Choose a valid native destination folder.'}</p>{/if}
    <div class="seg small">
      <button class:on={app.ws.layout === 'flat'} disabled={contextBusy || app.running || app.clonePreparing} onclick={() => changeContext(() => { app.ws.layout = 'flat'; })} title="Every repo directly under the root">Flat</button>
      <button class:on={app.ws.layout === 'custom'} disabled={contextBusy || app.running || app.clonePreparing} onclick={() => changeContext(() => { app.ws.layout = 'custom'; })} title="Build the folder path from a template">Custom</button>
    </div>
    {#if app.ws.layout === 'custom'}
      <div class="tplbox">
        <input class="big" bind:this={tplInput} value={app.ws.pathTemplate} onchange={event => changeInput(event, 'pathTemplate')} disabled={contextBusy || app.running || app.clonePreparing} placeholder={DEFAULT_TEMPLATE} spellcheck="false"
          title="Path below the root. Use \ to nest folders." />
        <div class="tokens">
          {#each PATH_TOKENS as t (t.token)}
            <button class="tok" title="Insert {t.token}: {t.hint}" onclick={() => insertToken(t.token)}>{t.token}</button>
          {/each}
        </div>
      </div>
    {/if}
  </div>
