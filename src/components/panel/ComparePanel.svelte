<script lang="ts">
  import { app } from '../../lib/state.svelte';
  import type { ShellTab } from '../../lib/workspace';
  import Icon from '../Icon.svelte';

  const subject = $derived(app.detailItem);
  const setBlocked = $derived(app.running || !app.set.items.length || app.set.items.some(item => item.path));
  const refsBlocked = $derived(app.running || !subject || !!subject.path || !app.local[app.dest(subject)]?.repo);
  const open = $derived(app.tabs.filter(tab => tab.view.kind === 'compare' || tab.view.kind === 'setCompare'));

  function describe(tab: ShellTab) {
    const view = tab.view;
    if (view.kind !== 'compare') return { title: app.tabTitle(tab), hint: 'Every repository in the set' };
    const side = view.left;
    const item = app.ws.sets.find(set => set.id === side.setId)?.items.find(entry => entry.id === side.itemId);
    return { title: item ? app.folderOf(item) : 'Repository', hint: view.readOnly ? 'Read-only refs' : 'Refs and working tree' };
  }
</script>

<div class="panel-body">
  <div class="sec">
    <h6>Start a comparison</h6>
    <button class="nav" disabled={setBlocked} title={setBlocked ? 'Needs a saved set with repositories, and no running Git operation' : 'Compare every repository in the set'} onclick={() => app.openSetCompare()}>
      <Icon name="copy" tone="inspect" /><span class="lbl">Compare across set</span>
    </button>
    <button class="nav" disabled={refsBlocked} title={refsBlocked ? 'Select one cloned repository first' : `Compare refs in ${app.folderOf(subject!)}`} onclick={() => app.openCompare(subject)}>
      <Icon name="branch" tone="inspect" /><span class="lbl">Compare repository refs</span>
    </button>
    {#if subject}<p class="mut panel-note">Repository: {app.folderOf(subject)}</p>{:else}<p class="mut panel-note">Select a repository in the table to compare its refs.</p>{/if}
  </div>
  <div class="sec">
    <h6>Open comparisons <small>{open.length}</small></h6>
    {#each open as tab (tab.id)}
      {@const info = describe(tab)}
      <div class="set-nav">
        <button class="nav" class:on={tab.id === app.activeTabId} onclick={() => app.activateTab(tab.id)}>
          <Icon name="copy" tone="brand" /><span class="lbl">{info.title}<small class="sub">{info.hint}</small></span>
        </button>
        <button class="set-nav-x" aria-label="Close {info.title} comparison" title="Close comparison" onclick={() => app.closeTab(tab.id)}><Icon name="close" size={12} /></button>
      </div>
    {:else}<div class="nav ghost">None open in this session</div>{/each}
  </div>
</div>
