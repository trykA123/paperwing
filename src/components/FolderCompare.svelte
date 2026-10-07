<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { benchmarkEnabled, benchmarkFinished } from '../lib/benchmark';
  import type { Source, Workspace, CompareFile } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import { commands, execute } from '../lib/commands';
  import { tabId } from '../lib/workspace';
  import type { View } from '../lib/workspace';
  import type { CompareState } from '../lib/compare.svelte';
  import { compareRows, directory, expandable, refLabel, sizeLabel, statusLabel, statusMark } from '../lib/compare-view';
  import CompareEndpointPicker from './CompareEndpointPicker.svelte';
  import Icon from './Icon.svelte';
  import EmptyState from './EmptyState.svelte';
  import Skeleton from './Skeleton.svelte';
  import VirtualList from './VirtualList.svelte';
  let { view, comparison }: { view: Extract<View, { kind: 'compare' }>; comparison: CompareState } = $props();
  const rows = $derived(compareRows(comparison.files, comparison.filter, comparison.query, comparison.excludes, comparison.expanded));
  const summary = $derived(comparison.snapshot?.display);
  const failure = $derived(comparison.error ?? (comparison.result?.status !== 'ready' ? comparison.result?.problem : null));
  let preparing = $state(false);
  let measured = false;
  $effect(() => {
    if (!benchmarkEnabled || measured || preparing || comparison.busy || !rows.length) return;
    measured = true;
    void tick().then(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())))).then(() => {
      if (!mounted || preparing || comparison.busy || !rows.length) return;
      const row = document.getElementById(`compare-row-${view.comparisonId}-${rows[0].id}`);
      if (!(row instanceof HTMLElement) || !row.isConnected || !row.getBoundingClientRect().height) return;
      row.click();
      if (comparison.selectedId !== rows[0].id) return;
      comparison.rendered();
      if (app.readonlyBenchmark) {
        const file = comparison.files.find(file => file.path === 'normalized/00000.txt');
        if (file) openFile(file);
      } else if (comparison.snapshot) void benchmarkFinished(comparison.snapshot, comparison.files);
    });
  });
  let mounted = true;
  let localError = $state('');
  let menu = $state<{ file: CompareFile; x: number; y: number } | null>(null);
  const selected = $derived(comparison.files.find(file => file.id === comparison.selectedId));
  const actions = $derived(commands());
  function command(id: string) { return actions.find(action => action.id === id)!; }
  $effect(() => {
    const snapshot = comparison.snapshot;
    if (snapshot) void app.probeEndpoints([snapshot.left.endpoint, snapshot.right.endpoint]);
  });
  $effect(() => {
    const identity = tabId(view, '');
    const snapshot = comparison.snapshot;
    const enabled = !comparison.busy && !preparing && !!snapshot && !!selected;
    const source = (side: 'left' | 'right') => !!selected?.[side] && !selected[side]?.reason && ['file', 'directory'].includes(selected[side]!.kind);
    app.copyActions[identity] = {
      leftReason: snapshot ? app.endpointCapability(snapshot.left.endpoint, 'copy').reason : null,
      rightReason: snapshot ? app.endpointCapability(snapshot.right.endpoint, 'copy').reason : null,
      left: enabled && !!snapshot && app.endpointCapability(snapshot.left.endpoint, 'copy').supported && snapshot?.left.endpoint.reference.kind === 'workingTree' && source('right'),
      right: enabled && !!snapshot && app.endpointCapability(snapshot.right.endpoint, 'copy').supported && snapshot?.right.endpoint.reference.kind === 'workingTree' && source('left'),
      copy: side => { if (selected) app.requestCopy(view.comparisonId, selected.id, side); },
    };
    return () => { delete app.copyActions[identity]; };
  });
  function focusMenu(element: HTMLElement) { element.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus(); }
  async function compare() {
    preparing = true; localError = '';
    try {
      await app.saveSettings({ sources: $state.snapshot(app.sources) as Source[], workspace: $state.snapshot(app.ws) as Workspace });
      if (!mounted) return;
      await comparison.open($state.snapshot(view.left), $state.snapshot(view.right), $state.snapshot(comparison.options));
      await comparison.loadAllFiles();
      if (comparison.mode === 'commits') await comparison.loadAllCommits();
    } catch (error) { localError = String(error); }
    finally { preparing = false; }
  }
  function openFile(file: CompareFile) {
    comparison.selectedId = file.id;
    const snapshot = comparison.snapshot;
    if (!directory(file) && snapshot) app.openView({ kind: 'fileDiff', comparisonId: view.comparisonId, fileId: file.id, path: file.path, sessionId: snapshot.id, generation: snapshot.generation });
  }
  function select(file: CompareFile) {
    comparison.selectedId = file.id;
    if (expandable(file)) comparison.expanded = comparison.expanded.includes(file.path) ? comparison.expanded.filter(path => path !== file.path) : [...comparison.expanded, file.path];
  }
  async function commits() { comparison.mode = 'commits'; await comparison.loadAllCommits(); }
  function next(event: KeyboardEvent) {
    event.stopPropagation();
    const current = rows.findIndex(file => file.id === comparison.selectedId);
    if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
      if (current < 0) return;
      event.preventDefault();
      const bounds = (event.currentTarget as HTMLElement).getBoundingClientRect();
      menu = { file: rows[current], x: Math.min(bounds.left + 16, innerWidth - 240), y: Math.min(bounds.top + 32, innerHeight - 160) };
      return;
    }
    if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
      if (current >= 0 && expandable(rows[current])) {
        event.preventDefault();
        const path = rows[current].path;
        comparison.expanded = event.key === 'ArrowRight' ? [...new Set([...comparison.expanded, path])] : comparison.expanded.filter(value => value !== path);
      }
      return;
    }
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp' && event.key !== 'Enter') return;
    event.preventDefault();
    if (event.key === 'Enter') { if (current >= 0) directory(rows[current]) ? select(rows[current]) : openFile(rows[current]); return; }
    const index = Math.max(0, Math.min(rows.length - 1, current + (event.key === 'ArrowDown' ? 1 : -1)));
    if (rows[index]) comparison.selectedId = rows[index].id;
    const box = (event.currentTarget as HTMLElement).closest('.compare-list')?.querySelector<HTMLElement>('.vbox');
    if (box) box.scrollTop = Math.max(index * 32 - box.clientHeight + 32, Math.min(index * 32, box.scrollTop));
  }
  onMount(() => { void compare(); return () => { mounted = false; }; });
</script>

<svelte:window onpointerdown={event => { if (!(event.target as Element).closest('.compare-menu')) menu = null; }} onkeydown={event => { if (event.key === 'Escape') menu = null; }} />
<section class="folder-compare">
  <div class="compare-bar">
    <div class="compare-sides">
      <CompareEndpointPicker side="LEFT" bind:endpoint={view.left} readOnly={view.readOnly} />
      <CompareEndpointPicker side="RIGHT" bind:endpoint={view.right} readOnly={view.readOnly} />
    </div>
    <div class="compare-actions">
      <button class="icon" title="Swap sides" aria-label="Swap sides" disabled={comparison.busy || preparing} onclick={() => { const left = view.left; view.left = view.right; view.right = left; void compare(); }}><Icon name="refresh" /></button>
      <button class="btn" disabled={comparison.busy || preparing} onclick={compare}><Icon name="refresh" /> Compare</button>
      {#if comparison.busy}<button class="icon" title="Cancel comparison" aria-label="Cancel comparison" onclick={() => comparison.cancel().catch(error => app.toast(String(error), 'error'))}><Icon name="close" /></button>{/if}
    </div>
  </div>
  <div class="compare-summary">
    <div class="seg"><button class:on={comparison.mode === 'files'} onclick={() => comparison.mode = 'files'}>Files</button><button class:on={comparison.mode === 'commits'} onclick={commits}>Commits</button></div>
    {#if comparison.snapshot?.history.available}<span class="compare-history">Right: <b>{comparison.snapshot.history.rightCount}</b> ahead, <b>{comparison.snapshot.history.leftCount}</b> behind{#if comparison.snapshot.history.leftBasis === 'workingTreeHead' || comparison.snapshot.history.rightBasis === 'workingTreeHead'}&nbsp;<span class="faint">(HEAD history)</span>{/if}</span>
    {:else if comparison.snapshot}<span class="faint" title={comparison.snapshot.history.reason ?? ''}>History: N/A</span>{/if}
    <span class="grow"></span>
    {#if summary}<span class="compare-different">{summary.different + summary.typeConflict} differ</span><span class="compare-leftOnly">{summary.leftOnly} only left</span><span class="compare-rightOnly">{summary.rightOnly} only right</span><span class="faint">{summary.same} identical</span>{#if summary.unavailable}<span class="warn">{summary.unavailable} unavailable</span>{/if}{/if}
  </div>
  {#if comparison.mode === 'files'}
    <div class="compare-filters"><div class="seg">{#each ['all', 'differences', 'same', 'orphans'] as filter}<button class:on={comparison.filter === filter} onclick={() => comparison.filter = filter as typeof comparison.filter}>{filter[0].toUpperCase() + filter.slice(1)}</button>{/each}</div>
      <button onclick={() => comparison.expanded = comparison.files.filter(expandable).map(file => file.path)}>Expand all</button><button onclick={() => comparison.expanded = []}>Collapse all</button>
      <input aria-label="Filter files" placeholder="Filter files (e.g. *.c)" bind:value={comparison.query} />
      <button class="btn" title={command('copy-left').reason ?? 'Copy selected file or folder to left'} disabled={!command('copy-left').enabled} onclick={() => execute(command('copy-left'))}><Icon name="copy" /> To left</button>
      <button class="btn" title={command('copy-right').reason ?? 'Copy selected file or folder to right'} disabled={!command('copy-right').enabled} onclick={() => execute(command('copy-right'))}><Icon name="copy" /> To right</button>
    </div>
  {/if}
  {#if preparing || comparison.busy}<div class="compare-loading" role="status"><span class="sr-only">Comparing…</span><Skeleton rows={9} height={32} /></div>
  {:else if failure || localError}<div class="compare-message" role="status"><b>{comparison.result?.status === 'unavailable' ? 'Not available' : 'Comparison could not complete'}</b><p>{failure?.message ?? localError}</p><button class="btn" onclick={compare}>Retry</button></div>
  {:else if comparison.snapshot && comparison.mode === 'commits'}
    <div class="compare-commits">
      {#if !comparison.snapshot.history.available}<p class="compare-message">{comparison.snapshot.history.reason ?? 'History is not available for these repositories.'}</p>
      {:else}{#each ['right', 'left'] as side}<h3>{side === 'right' ? comparison.snapshot.history.rightCount : comparison.snapshot.history.leftCount} commits only in {refLabel(side === 'right' ? comparison.snapshot.right.endpoint.reference : comparison.snapshot.left.endpoint.reference)}</h3>
        {#each comparison.commits.filter(commit => commit.side === side) as commit}<div class="compare-commit"><Icon name="commit" tone="commit" /><span class="grow">{commit.subject}</span><span class="mut">{commit.author}</span><time title={commit.date}>{commit.date.slice(0, 10)}</time><span class="mono faint">{commit.sha.slice(0, 8)}</span></div>{/each}
      {/each}{/if}
    </div>
  {:else if comparison.snapshot}
    <div class="compare-pathbar"><span>{refLabel(comparison.snapshot.left.endpoint.reference)}</span><span></span><span>{refLabel(comparison.snapshot.right.endpoint.reference)}</span></div>
    <div class="compare-column-head"><span>Name</span><span>Size</span><span>Modified</span><span></span><span>Name</span><span>Size</span><span>Modified</span></div>
    <div class="compare-list" role="grid" aria-label="Compared files" tabindex="0" onkeydown={next} aria-activedescendant={comparison.selectedId ? `compare-row-${view.comparisonId}-${comparison.selectedId}` : undefined}>
      {#if !rows.length}<EmptyState icon="search" title="No files match" hint="Adjust the filter or switch the view to All." />{/if}
      <VirtualList items={rows} rowHeight={32} key={(file: CompareFile) => file.id}>
        {#snippet row(file: CompareFile)}
          <div role="row" tabindex="-1" onkeydown={next} id="compare-row-{view.comparisonId}-{file.id}" class="compare-row compare-{file.displayStatus}" class:selected={comparison.selectedId === file.id}
            aria-selected={comparison.selectedId === file.id} style:--depth={file.path.split('/').length - 1}
            onclick={() => select(file)} ondblclick={() => openFile(file)}
            oncontextmenu={event => { event.preventDefault(); comparison.selectedId = file.id; menu = { file, x: Math.max(8, Math.min(event.clientX, innerWidth - 240)), y: Math.max(8, Math.min(event.clientY, innerHeight - 160)) }; }}>
            {#each ['left', 'right'] as side, index}
              {#if index === 1}<span role="gridcell" class="compare-marker" title={statusLabel[file.displayStatus]}>{statusMark[file.displayStatus]}</span>{/if}
              {@const data = side === 'left' ? file.left : file.right}
              <span role="gridcell" class="compare-name" class:absent={!data} title={file.path}>
                {#if data}{#if directory(file)}<span class="compare-twisty" class:expanded={comparison.expanded.includes(file.path)}><Icon name="disclosure" size={12} /></span><Icon name="folder" tone="folder" />{:else}<span class="compare-indent"></span><Icon name="code" tone="file" />{/if}<span class="compare-filename">{file.path.split('/').at(-1)}</span>
                {#if file.displayLines && file.displayStatus !== 'same'}<span class="compare-line-count"><span class="ok">+{file.displayLines.added}</span> <span class="err">-{file.displayLines.removed}</span></span>{/if}{/if}
              </span>
              <span role="gridcell" class="compare-number">{data ? sizeLabel(data.size) : ''}</span>
              <span role="gridcell" class="compare-number" title={data?.modifiedMs ? new Date(data.modifiedMs).toLocaleString() : 'Historical file dates are not available'}>{data?.modifiedMs ? new Date(data.modifiedMs).toLocaleDateString() : data ? '\u2014' : ''}</span>
            {/each}
          </div>
        {/snippet}
      </VirtualList>
    </div>
  {/if}
</section>
{#if menu}<div class="row-menu compare-menu" use:focusMenu style:left="{menu.x}px" style:top="{menu.y}px" role="menu" tabindex="-1">
  <button role="menuitem" disabled={directory(menu.file)} onclick={() => { if (menu) openFile(menu.file); menu = null; }}><Icon name="code" /> Open diff</button>
  <button role="menuitem" title={command('copy-left').reason ?? undefined} disabled={!command('copy-left').enabled} onclick={() => { execute(command('copy-left')); menu = null; }}><Icon name="copy" /> Copy to left</button>
  <button role="menuitem" title={command('copy-right').reason ?? undefined} disabled={!command('copy-right').enabled} onclick={() => { execute(command('copy-right')); menu = null; }}><Icon name="copy" /> Copy to right</button>
</div>{/if}