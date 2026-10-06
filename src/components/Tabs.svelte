<script lang="ts">
  import { app } from '../lib/state.svelte';
  import Icon from './Icon.svelte';
  import WindowControls from './WindowControls.svelte';

  function navigate(event: KeyboardEvent, index: number) {
    let next = index;
    if (event.key === 'ArrowRight') next = (index + 1) % app.tabs.length;
    else if (event.key === 'ArrowLeft') next = (index + app.tabs.length - 1) % app.tabs.length;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = app.tabs.length - 1;
    else return;
    event.preventDefault();
    app.activateTab(app.tabs[next].id);
    (event.currentTarget as HTMLElement).closest('.tabstrip')?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[next]?.focus();
  }

  const titlebar = $derived(app.platform.platform === 'windows' ? true : undefined);
</script>

<header class="tabs-chrome" class:native-controls={titlebar} data-tauri-drag-region={titlebar}>
  <div class="tabstrip" role="tablist" aria-label="Workspace tabs">
    {#each app.tabs as tab, index (tab.id)}
      <div class="shell-tab" class:on={tab.id === app.activeTabId}>
        <button role="tab" aria-selected={tab.id === app.activeTabId} aria-controls="workspace-view"
          tabindex={tab.id === app.activeTabId ? 0 : -1} title={app.tabTitle(tab)}
          onclick={() => app.activateTab(tab.id)} onkeydown={event => navigate(event, index)}>
          <Icon name={tab.view.kind === 'compare' || tab.view.kind === 'setCompare' ? 'copy' : tab.view.kind === 'fileDiff' ? 'code' : tab.view.kind === 'settings' ? 'gear' : tab.view.kind === 'search' || tab.view.kind === 'org' ? 'search' : 'folder'}
            tone={tab.view.kind === 'compare' || tab.view.kind === 'setCompare' ? 'brand' : tab.view.kind === 'fileDiff' ? 'file' : tab.view.kind === 'set' || tab.view.kind === 'item' ? 'folder' : undefined} />
          <span>{app.tabTitle(tab)}</span>
        </button>
        <button class="tab-close" title="Close {app.tabTitle(tab)} (Ctrl+W)" aria-label="Close {app.tabTitle(tab)}" onclick={() => app.closeTab(tab.id)}><Icon name="close" size={12} /></button>
      </div>
    {/each}
  </div>
  <div class="drag-spacer" data-tauri-drag-region={titlebar}></div>
  <div class="topbtns">
    <button class="kbtn" title="Command palette (Ctrl+K)" onclick={() => (app.paletteOpen = true)}><Icon name="search" /><span>Search or run a command...</span><kbd>Ctrl K</kbd></button>
    <button class="icon shell-control" class:on={app.ws.shell.sidebarVisible} title="Toggle sidebar" aria-label="Toggle sidebar" aria-pressed={app.ws.shell.sidebarVisible}
      onclick={() => (app.ws.shell.sidebarVisible = !app.ws.shell.sidebarVisible)}><Icon name="panel" /></button>
    <button class="icon shell-control" class:on={app.ws.shell.rightVisible} disabled={app.view.kind === 'settings'} title="Toggle details" aria-label="Toggle details" aria-pressed={app.ws.shell.rightVisible}
      onclick={() => (app.ws.shell.rightVisible = !app.ws.shell.rightVisible)}><Icon name="panel" /></button>
    <button class="icon shell-control" title="Toggle light/dark theme" aria-label="Toggle light/dark theme"
      onclick={() => (app.ws.theme = app.ws.theme === 'dark' ? 'light' : 'dark')}><Icon name="theme" /></button>
  </div>
  <WindowControls />
</header>

<style>
  .tabs-chrome.native-controls {
    padding-right: 0;
  }

  .drag-spacer {
    flex: none;
    align-self: stretch;
    width: var(--drag-min);
  }
</style>
