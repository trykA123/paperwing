<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { app, DEFAULT_TEMPLATE, PATH_TOKENS, RUNNING } from '../../lib/state.svelte';
  import type { SetItem } from '../../lib/api';
  import Icon from '../Icon.svelte';

  let { items }: { items: SetItem[] } = $props();

  function parts(item: SetItem) {
    const segments = app.segments(item);
    return { dir: segments.length > 1 ? `${segments.slice(0, -1).join(app.platform.separator)}${app.platform.separator}` : '', leaf: segments.at(-1) ?? '' };
  }
  function tag(item: SetItem): { text: string; tone: 'new' | 'ok' | 'warn' | 'err' } {
    if (app.hasClash(item)) return { text: 'name clash', tone: 'err' };
    if (app.refState(item) === 'missing') return { text: 'ref not found', tone: 'warn' };
    if (app.exists[app.dest(item)]) return { text: `exists \u00b7 ${EXISTS[app.ws.onExisting]}`, tone: app.ws.onExisting === 'reclone' ? 'warn' : 'ok' };
    return { text: 'new', tone: 'new' };
  }
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
  const EXISTS = { fetch: 'fetch', skip: 'skip', reclone: 're-clone' } as const;

  function dot(i: SetItem) {
    const j = app.jobs[i.id];
    if (j) return RUNNING.includes(j.phase) ? 'd-run' : j.phase === 'queued' ? '' : `d-${j.phase}`;
    if (app.hasClash(i)) return 'd-failed';
    return app.refState(i) === 'missing' ? 'd-warn' : '';
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

    <div class="rsec">
      <h3 class="hrow"><span>Will be created <small>{items.length}</small></span></h3>
      {#if items.length}
        <ul class="plan">
          {#each items as item (item.id)}
            {@const place = parts(item)}
            {@const state = tag(item)}
            <li class="plan-row">
              <span class="dot {dot(item)}"></span>
              <span class="plan-path" title={app.dest(item)}><span class="mut">{place.dir}</span><b>{place.leaf}</b>{#if item.folder}<span class="mut"> ({item.name})</span>{/if}</span>
              <span class="plan-ref t-{item.ref.type}"><Icon name={item.ref.type} size={12} />{app.refLabel(item)}</span>
              <span class="plan-tag {state.tone}">{state.text}</span>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="mut">Nothing selected. Tick repositories in the table to see where they will be cloned.</p>
      {/if}
    </div>

    <div class="rsec">
      <h3>Options</h3>
      <div class="opt-summary">
        <span>{app.ws.shallow ? 'Shallow' : 'Full history'} · {app.ws.parallel} parallel · existing: {EXISTS[app.ws.onExisting]}</span>
        <button class="link" onclick={() => app.openView({ kind: 'settings' })}>Change in Settings</button>
      </div>
    </div>
