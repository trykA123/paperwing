<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { app } from '../lib/state.svelte';
  import Icon from './Icon.svelte';

  let maximized = $state(false);

  $effect(() => {
    if (app.platform.platform !== 'windows') return;
    const win = getCurrentWindow();
    let disposed = false;
    let stop: (() => void) | undefined;
    const sync = () => win.isMaximized().then(value => { if (!disposed) maximized = value; }).catch(() => {});
    void sync();
    win.onResized(sync).then(off => { if (disposed) off(); else stop = off; }).catch(() => {});
    return () => { disposed = true; stop?.(); };
  });

  const act = (run: (win: ReturnType<typeof getCurrentWindow>) => Promise<void>) => () => { void run(getCurrentWindow()).catch(() => {}); };
</script>

{#if app.platform.platform === 'windows'}
  <div class="window-controls">
    <button type="button" title="Minimize" aria-label="Minimize" onclick={act(win => win.minimize())}><Icon name="minimize" size={16} /></button>
    <button type="button" title={maximized ? 'Restore' : 'Maximize'} aria-label={maximized ? 'Restore' : 'Maximize'} onclick={act(win => win.toggleMaximize())}><Icon name={maximized ? 'restore' : 'maximize'} size={16} /></button>
    <button type="button" class="close" title="Close" aria-label="Close" onclick={act(win => win.close())}><Icon name="close" size={16} /></button>
  </div>
{/if}

<style>
  .window-controls {
    --win-close: #c42b1c;
    display: flex;
    align-self: stretch;
    flex: none;
  }

  button {
    display: grid;
    place-items: center;
    width: 46px;
    height: 100%;
    border-radius: 0;
    color: var(--text);
  }

  button:hover {
    background: var(--soft);
  }

  button:focus-visible {
    outline-offset: -2px;
  }

  button.close:hover {
    background: var(--win-close);
    color: #fff;
  }
</style>
