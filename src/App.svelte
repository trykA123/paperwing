<script lang="ts">
  import { onMount } from 'svelte';
  import { benchmarkEnabled } from './lib/benchmark';
  import { fly } from 'svelte/transition';
  import { isTauri } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { api, type Source, type Workspace } from './lib/api';
  import { app } from './lib/state.svelte';
  import { commands, execute, shortcut } from './lib/commands';
  import { applyAppearance, onSystemThemeChange } from './lib/appearance';
  import SetView from './components/SetView.svelte';
  import RepoList from './components/RepoList.svelte';
  import RightPanel from './components/RightPanel.svelte';
  import Settings from './components/Settings.svelte';
  import Notifications from './components/Notifications.svelte';
  import Tooltip from './components/Tooltip.svelte';
  import Tabs from './components/Tabs.svelte';
  import CommandPalette from './components/CommandPalette.svelte';
  import Icon from './components/Icon.svelte';
  import FolderCompare from './components/FolderCompare.svelte';
  import CompareDetails from './components/CompareDetails.svelte';
  import FileCompare from './components/FileCompare.svelte';
  import CopyOperations from './components/CopyOperations.svelte';
  import RecoveryPanel from './components/RecoveryPanel.svelte';
  import CommitDialog from './components/CommitDialog.svelte';
  import BranchDialog from './components/BranchDialog.svelte';
  import ConfirmDialog from './components/ConfirmDialog.svelte';
  import HistoryDrawer from './components/HistoryDrawer.svelte';
  import { historyDrawer } from './lib/history-drawer.svelte';
  import { confirmQueue } from './lib/confirm';
  import SetCompare from './components/SetCompare.svelte';
  import BranchCleanup from './components/BranchCleanup.svelte';
  import CodeSearch from './components/CodeSearch.svelte';
  import ActivityRail from './components/ActivityRail.svelte';
  import SidePanel from './components/SidePanel.svelte';
  import StashPushDialog from './components/stash/StashPushDialog.svelte';
  import StashSwitchDialog from './components/stash/StashSwitchDialog.svelte';
  import TagDeleteDialog from './components/tags/TagDeleteDialog.svelte';
  import TagDialog from './components/tags/TagDialog.svelte';
  import { stashFlow } from './lib/stash-flow.svelte';
  import { tagFlow } from './lib/tag-flow.svelte';
  import { RAIL_SECTIONS, railShortcut } from './lib/rail';

  const rightVisible = $derived(app.ws.shell.rightVisible && app.view.kind !== 'settings' && app.view.kind !== 'codeSearch');
  const failedRuns = $derived(app.activity.filter(entry => entry.state === 'failed' || entry.state === 'timedOut').length);
  let reducedMotion = $state(false), panelsMoving = $state(false);
  const gitBusy = $derived(app.running || app.gitBusy || app.clonePreparing || app.activity.some(entry => entry.state === 'running'));
  let previousPanels: string | undefined;
  $effect.pre(() => {
    const visibility = `${app.ws.shell.sidebarVisible}:${rightVisible}`;
    if (previousPanels === visibility) return;
    const changed = previousPanels !== undefined;
    previousPanels = visibility;
    if (!changed) return;
    panelsMoving = true;
    const timer = setTimeout(() => { panelsMoving = false; }, 220);
    return () => clearTimeout(timer);
  });

  function onKey(event: KeyboardEvent) {
    if (app.copyRequest || app.recoveryOpen || historyDrawer.target || document.querySelector('dialog[open]:not(.palette)')) return;
    if (event.ctrlKey && !event.altKey && !event.shiftKey && event.key.toLowerCase() === 'k') {
      event.preventDefault();
      app.paletteOpen = !app.paletteOpen;
      event.stopPropagation();
      return;
    }
    const id = shortcut(event);
    if (!id) return;
    if (id.startsWith('rail-')) {
      if (!app.ready || app.paletteOpen) return;
      event.preventDefault(); event.stopPropagation();
      const target = railShortcut(id.slice(5));
      if (target === 'settings') app.openView({ kind: 'settings' });
      else if (target) Object.assign(app.ws.shell, { section: target, sidebarVisible: true });
      return;
    }
    if (id.startsWith('tab-') || id === 'search-code') {
      if (app.paletteOpen || !app.ready) return;
      event.preventDefault(); event.stopPropagation();
      if (id === 'tab-close' && event.repeat) return;
      const command = commands().find(command => command.id === id);
      if (command) execute(command);
      return;
    }
    if (app.view.kind !== 'fileDiff' || app.paletteOpen) return;
    const target = event.target as HTMLElement;
    if (target.closest('[role="dialog"], .compare-menu, .row-menu')) return;
    if (target.closest('input, textarea, select, [contenteditable="true"]') && !target.closest('.monaco-editor')) return;
    const command = commands().find(command => command.id === id);
    if (!command?.enabled) return;
    event.preventDefault(); event.stopPropagation(); execute(command);
  }

  onMount(() => {
    const motion = matchMedia('(prefers-reduced-motion: reduce)');
    const updateMotion = () => { reducedMotion = motion.matches; };
    updateMotion(); motion.addEventListener('change', updateMotion);
    window.addEventListener('keydown', onKey, true);
    app.init().catch(e => app.toast(`Could not load settings: ${e}`, 'error'));
    if (isTauri()) app.temporary.start().catch(e => app.toast(`Could not listen for launch requests: ${e}`, 'error'));
    let disposed = false, closing = false;
    let unlisten: (() => void) | undefined;
    if (isTauri()) getCurrentWindow().onCloseRequested(async event => {
      event.preventDefault();
      if (closing) return;
      closing = true;
      try {
        if (!await app.guardBuffers()) return;
        if (app.ready) await api.saveSettings({ sources: $state.snapshot(app.sources), workspace: $state.snapshot(app.ws) });
        await getCurrentWindow().destroy();
      } catch (reason) { app.toast(String(reason), 'error'); } finally { closing = false; }
    }).then(stop => { if (disposed) stop(); else unlisten = stop; }).catch(reason => app.toast(String(reason), 'error'));
    return () => { disposed = true; unlisten?.(); app.temporary.stop(); window.removeEventListener('keydown', onKey, true); motion.removeEventListener('change', updateMotion); };
  });

  $effect(() => applyAppearance(app.ws.theme, app.ws.uiFont, app.ws.codeFont));
  $effect(() => onSystemThemeChange(() => applyAppearance(app.ws.theme, app.ws.uiFont, app.ws.codeFont)));

  // Branches may have been switched in a terminal meanwhile; mark remote metadata stale when the window regains focus.
  function onFocus() {
    if (!benchmarkEnabled && app.ready && !app.running) {
      app.markMetadataStale();
      void app.checkExists(app.set.items.map(i => app.dest(i)));
    }
  }

  // Persist sources + workspace shortly after any change.
  $effect(() => {
    if (!app.ready) return;
    const data = { sources: $state.snapshot(app.sources) as Source[], workspace: $state.snapshot(app.ws) as Workspace };
    const t = setTimeout(() => api.saveSettings(data).catch(e => app.toast(`Could not save settings: ${e}`, 'error')), 400);
    return () => clearTimeout(t);
  });

  $effect(() => {
    if (!app.ready) return;
    const root = app.ws.root;
    void app.probeRoot(root);
  });

  // Keep "already on disk" markers in sync with the destination.
  $effect(() => {
    if (benchmarkEnabled || !app.ready || !(app.rootSupport.valid || app.set.items.some(item => item.path))) return;
    const dests = app.set.items.map(i => app.dest(i));
    const t = setTimeout(() => {
      void app.checkExists(dests);
      void app.refreshPathIdentities(dests).catch(reason => app.toast(String(reason), 'warn'));
    }, 300);
    return () => clearTimeout(t);
  });
</script>

<svelte:window onfocus={onFocus} />

<div id="shell" class:noright={!rightVisible} class:noside={!app.ws.shell.sidebarVisible} class:panels-moving={panelsMoving}
  style:--lw="{app.ws.shell.sidebarVisible ? app.ws.shell.sidebarWidth : 0}px" style:--rw="{rightVisible ? app.ws.rightWidth : 0}px">
  <ActivityRail {gitBusy} />
  <div class="shell-brand" data-tauri-drag-region={app.platform.platform === 'windows' ? 'deep' : undefined}><span>{RAIL_SECTIONS.find(entry => entry.id === app.ws.shell.section)?.label}</span></div>
  <Tabs />
  <div class="shell-side" inert={!app.ws.shell.sidebarVisible} aria-hidden={!app.ws.shell.sidebarVisible} style:--panel-width="{app.ws.shell.sidebarWidth}px">
    {#if app.ws.shell.sidebarVisible}<div class="shell-panel-content" transition:fly={{ x: -12, duration: reducedMotion ? 0 : 180 }}><SidePanel /></div>{/if}
  </div>
  <main id="workspace-view" class="main" class:scroll={app.view.kind === 'settings'}>
    {#if !app.ready}
      <div class="empty"><span class="spin"></span></div>
    {:else if app.view.kind === 'set' || app.view.kind === 'item'}
      {#key app.activeTabId}<SetView />{/key}
    {:else if app.view.kind === 'org'}
      {#key app.activeTabId}
        <RepoList mode="org" source={app.view.source} org={app.view.org} />
      {/key}
    {:else if app.view.kind === 'search'}
      {#key app.activeTabId}<RepoList mode="search" />{/key}
    {:else if app.view.kind === 'settings'}
      <Settings />
    {/if}
    {#each app.tabs as tab (tab.id)}
      {#if tab.view.kind === 'setCompare' && app.setComparisons[tab.view.comparisonId]}<div class="compare-tab" hidden={tab.id !== app.activeTabId}><SetCompare comparison={app.setComparisons[tab.view.comparisonId]} /></div>
      {:else if tab.view.kind === 'codeSearch' && app.codeSearches[tab.id]}<div class="compare-tab" hidden={tab.id !== app.activeTabId}><CodeSearch session={app.codeSearches[tab.id]} setId={tab.setId} /></div>
      {:else if tab.view.kind === 'compare' && app.comparisons[tab.view.comparisonId]}<div class="compare-tab" hidden={tab.id !== app.activeTabId}><FolderCompare view={tab.view} comparison={app.comparisons[tab.view.comparisonId]} /></div>
      {:else if tab.view.kind === 'fileDiff' && app.comparisons[tab.view.comparisonId]}<div class="compare-tab" hidden={tab.id !== app.activeTabId}><FileCompare view={tab.view} comparison={app.comparisons[tab.view.comparisonId]} active={tab.id === app.activeTabId} /></div>{/if}
    {/each}
  </main>
  <div class="shell-right" inert={!rightVisible} aria-hidden={!rightVisible} style:--panel-width="{app.ws.rightWidth}px">
    {#if rightVisible}<div class="shell-panel-content" transition:fly={{ x: 12, duration: reducedMotion ? 0 : 180 }}>{#if (app.view.kind === 'compare' || app.view.kind === 'fileDiff') && app.comparisons[app.view.comparisonId]}<CompareDetails comparison={app.comparisons[app.view.comparisonId]} comparisonId={app.view.comparisonId} />{:else}<RightPanel />{/if}</div>{/if}
  </div>
  <footer class="shell-status" class:busy={app.running}><span class="status-context"><Icon name="folder" tone="folder" />{app.set.name} · {app.set.items.length} repositories{#if app.focusedItem} · {app.folderOf(app.focusedItem)}{/if}</span>
    <span class="grow"></span>
    {#if app.running}
      <span class="status-progress" role="progressbar" aria-label="Git operation progress" aria-valuemin="0" aria-valuemax="100" aria-valuenow={app.runProgress.pct}>
        <span class="bar"><i style:width="{app.runProgress.pct}%"></i></span>{app.runProgress.finished}/{app.runProgress.total}
      </span>
    {:else}<span><span class="dot d-done"></span>Ready</span>{/if}
    <button title="Toggle Git activity" aria-expanded={app.activityOpen} onclick={() => (app.activityOpen = !app.activityOpen)}><Icon name="activity" /> Activity · {app.activity.length}{#if failedRuns}<span class="badge-err">{failedRuns} failed</span>{/if}</button></footer>
  {#if historyDrawer.target}{#key historyDrawer.target.path}<HistoryDrawer target={historyDrawer.target} />{/key}{/if}
</div>
{#if app.paletteOpen}<CommandPalette />{/if}
{#if app.copyRequest}<CopyOperations request={app.copyRequest} />{/if}
{#if app.recoveryOpen}<RecoveryPanel />{/if}
{#if app.gitDialog?.kind === 'commit'}<CommitDialog request={app.gitDialog} />{:else if app.gitDialog?.kind === 'branch'}<BranchDialog request={app.gitDialog} />{/if}
{#if app.cleanupDialog}<BranchCleanup request={app.cleanupDialog} />{/if}
{#if stashFlow.dialog?.kind === 'push'}<StashPushDialog targets={stashFlow.dialog.targets} />{:else if stashFlow.dialog?.kind === 'switch'}<StashSwitchDialog targets={stashFlow.dialog.targets} />{/if}
{#if tagFlow.dialog?.kind === 'create'}<TagDialog targets={tagFlow.dialog.targets} />{:else if tagFlow.dialog?.kind === 'delete'}<TagDeleteDialog targets={tagFlow.dialog.targets} tag={tagFlow.dialog.tag} />{/if}
{#if $confirmQueue.length}{#key $confirmQueue[0]}<ConfirmDialog request={$confirmQueue[0]} />{/key}{/if}
<Notifications />
<Tooltip />
