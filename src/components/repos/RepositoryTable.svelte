<script lang="ts">
  import { tick, untrack, type Snippet } from 'svelte';
  import { app } from '../../lib/state.svelte';
  import { detailsDrawer } from '../../lib/details-drawer.svelte';
  import type { SetItem } from '../../lib/api';
  import { clearRange, selectionOffer, shiftRange } from '../../lib/selection';
  import { describeRow } from '../../lib/formation-row';
  import { pullKey } from '../../lib/pull-flow.svelte';
  import { pulls } from '../../lib/pulls.svelte';
  import { runNextAction } from '../../lib/row-actions';
  import VirtualList from '../VirtualList.svelte';
  import Pager from '../Pager.svelte';
  import Icon from '../Icon.svelte';
  import FormationRow from '../set/FormationRow.svelte';
  import RowMenu from '../set/RowMenu.svelte';
  import AddToSetMenu from './AddToSetMenu.svelte';
  import RepositoryBulk from './RepositoryBulk.svelte';

  let { items, label, scopeKey, none }: { items: SetItem[]; label: string; scopeKey: string; none: Snippet } = $props();

  const store = app.repositories;
  let editingId = $state<string | null>(null);
  let list = $state<ReturnType<typeof VirtualList<SetItem>>>();
  let bulk = $state<ReturnType<typeof RepositoryBulk>>();
  let activeId = $state<string | null>(null);
  let anchorId: string | null = null;
  let rangeAdded = new Set<string>();
  let menu = $state<{ item: SetItem; x: number; y: number; opener: HTMLElement | null } | null>(null);
  let addTo = $state<{ item: SetItem; anchor: Element } | null>(null);
  let menuNeighbour: string | undefined;

  const size = $derived(app.ws.pageSize);
  const pages = $derived(size === 'all' ? 1 : Math.max(1, Math.ceil(items.length / size)));
  const cur = $derived(Math.min(store.page, pages - 1));
  const rows = $derived(size === 'all' ? items : items.slice(cur * size, cur * size + size));
  const density = $derived(app.ws.density ?? 'comfortable');
  const activeRow = $derived(rows.find(item => item.id === activeId) ?? rows[0]);
  const selected = $derived(store.selected);
  const allOn = $derived(rows.length > 0 && rows.every(item => item.on));
  const offer = $derived(selectionOffer(rows, items));
  const listKey = $derived(`${scopeKey}|${store.chip}|${store.query}|${store.hostFilter}|${store.org}|${cur}|${size}`);
  const firstKey = untrack(() => listKey), firstScroll = untrack(() => store.scroll);
  const gitBusy = $derived(app.running || app.gitBusy || app.clonePreparing);

  $effect(() => { pulls.pin(selected.flatMap(item => pullKey(item) ?? [])); });
  $effect(() => { scopeKey; menu = null; });

  // A freshly duplicated row opens straight into folder rename, on whichever page it landed.
  $effect(() => {
    const id = app.renameItemId;
    if (!id) return;
    app.renameItemId = null;
    const k = untrack(() => items.findIndex(item => item.id === id));
    if (k >= 0 && size !== 'all') store.page = Math.floor(k / size);
    editingId = id;
  });

  function openMenu(item: SetItem, anchor: HTMLElement | { x: number; y: number; opener?: HTMLElement }) {
    const at = rows.indexOf(item);
    menuNeighbour = (rows[at + 1] ?? rows[at - 1])?.id;
    if (anchor instanceof HTMLElement) {
      const box = anchor.getBoundingClientRect();
      menu = { item, x: Math.max(8, Math.min(box.right - 290, innerWidth - 300)), y: Math.max(8, Math.min(box.bottom + 4, innerHeight - 420)), opener: anchor };
    } else menu = { item, x: Math.max(8, Math.min(anchor.x, innerWidth - 300)), y: Math.max(8, Math.min(anchor.y, innerHeight - 420)), opener: anchor.opener ?? null };
  }

  function rename(item: SetItem, value: string | null) {
    if (editingId !== item.id) return;
    if (value !== null) app.renameFolder(item, value);
    editingId = null;
  }

  const PAGE_STEP = 10;
  const ids = () => items.map(item => item.id);

  function anchorAt(id: string | null) {
    anchorId = id;
    rangeAdded = new Set();
  }

  function extendTo(target: SetItem) {
    anchorId ??= activeRow?.id ?? target.id;
    const byId = new Map(items.map(item => [item.id, item]));
    const result = shiftRange(ids(), anchorId, target.id, id => !!byId.get(id)?.on, rangeAdded);
    for (const id of result.on) byId.get(id)!.on = true;
    for (const id of result.off) byId.get(id)!.on = false;
    rangeAdded = result.added;
  }

  async function focusRow(index: number, extend: boolean) {
    const target = rows[Math.max(0, Math.min(rows.length - 1, index))];
    if (!target) return;
    if (extend) extendTo(target); else anchorAt(target.id);
    activeId = target.id;
    list?.reveal(rows.indexOf(target));
    await tick();
    document.querySelector<HTMLElement>(`.fm-table .fm-row[data-id="${CSS.escape(target.id)}"]`)?.focus();
  }

  function gridKey(event: KeyboardEvent) {
    if (!activeRow) return;
    const row = (event.target as Element).closest<HTMLElement>('.fm-row[data-id]');
    const onGrid = event.currentTarget === event.target;
    if (event.target !== row && !onGrid) return;
    const at = rows.indexOf(activeRow);
    const control = event.ctrlKey || event.metaKey;
    if (onGrid) {
      if (!['ArrowDown', 'ArrowUp', 'PageDown', 'PageUp', 'Home', 'End', 'Enter', ' '].includes(event.key)) return;
      event.preventDefault();
      void focusRow(at, false);
      return;
    }
    if (control && event.key.toLowerCase() === 'a') { for (const item of rows) item.on = true; }
    else if (event.key === 'ArrowDown') void focusRow(at + 1, event.shiftKey);
    else if (event.key === 'ArrowUp') void focusRow(at - 1, event.shiftKey);
    else if (event.key === 'PageDown') void focusRow(at + PAGE_STEP, event.shiftKey);
    else if (event.key === 'PageUp') void focusRow(at - PAGE_STEP, event.shiftKey);
    else if (event.key === 'Home') void focusRow(0, event.shiftKey);
    else if (event.key === 'End') void focusRow(rows.length - 1, event.shiftKey);
    else if (event.key === ' ' && !control) detailsDrawer.open({ kind: 'repository', item: activeRow }, event.target as Element);
    else if (event.key === ' ' || event.key.toLowerCase() === 'x') { activeRow.on = !activeRow.on; anchorAt(activeRow.id); }
    else if (event.key === 'Enter') store.openRepository(activeRow.repoId);
    else return;
    event.preventDefault();
  }

  function removed(id: string, neighbour: string | undefined) {
    if (app.ws.sets.some(set => set.items.some(item => item.id === id))) return;
    const target = rows.find(item => item.id === neighbour) ?? rows[0];
    if (!target) { document.querySelector<HTMLElement>('.fm-table .vbox')?.focus(); return; }
    void focusRow(rows.indexOf(target), false);
  }

  const rowHandlers = (item: SetItem) => ({
    toggle: (on: boolean, range: boolean) => {
      if (range && anchorId) {
        if (on) extendTo(item);
        else { const byId = new Map(items.map(entry => [entry.id, entry])); for (const id of clearRange(ids(), anchorId, item.id)) byId.get(id)!.on = false; rangeAdded = new Set(); }
      } else { item.on = on; anchorAt(item.id); }
    },
    activate: () => { activeId = item.id; },
    inspect: () => detailsDrawer.open({ kind: 'repository', item }),
    open: () => store.openRepository(item.repoId),
    pickRef: (anchor: HTMLElement) => bulk?.pickRef([item], anchor.getBoundingClientRect()),
    next: (kind: Parameters<typeof runNextAction>[1]) => runNextAction(item, kind),
    menu: (anchor: HTMLElement | { x: number; y: number; opener?: HTMLElement }) => openMenu(item, anchor),
    rename: (value: string | null) => rename(item, value),
    retryStatus: () => { void app.checkExists([app.dest(item)]); },
  });
</script>

<div class="fm-wrap">
  <div class="card fill repository-table fm-table" class:compact={density === 'compact'} class:running={app.running || app.clonePreparing}>
    {#key listKey}
      <VirtualList bind:this={list} role="grid" activeKey={activeRow?.id} {label} onkeydown={gridKey} items={rows} rowHeight={density === 'compact' ? 40 : 56} key={i => i.id} initialScroll={listKey === firstKey ? firstScroll : 0} onscrolled={top => (store.scroll = top)}>
        {#snippet header()}
          <div class="fm-row fm-head" role="row">
            <div class="fm-cell fm-check" role="columnheader"><label class="fm-hit"><input type="checkbox" checked={allOn} indeterminate={!allOn && rows.some(item => item.on)}
              onchange={e => { for (const item of rows) item.on = e.currentTarget.checked; }} aria-label="Select all repositories on this page" /></label></div>
            <div class="fm-cell" role="columnheader">Repository</div><div class="fm-cell" role="columnheader">Branch</div>
            <div class="fm-cell fm-pull-head" role="columnheader"><span>Pull request</span><button class="fm-menu-btn" aria-label="Refresh pull request status" title="Check pull requests again for the rows shown and selected" onclick={() => pulls.refresh()}><Icon name="refresh" size={12} /></button></div>
            <div class="fm-cell" role="columnheader">Sync</div><div class="fm-cell" role="columnheader">Next action</div><div class="fm-cell" role="columnheader"><span class="sr-only">Actions</span></div>
          </div>
        {/snippet}
        {#snippet empty()}{@render none()}{/snippet}
        {#snippet row(item: SetItem, index: number)}
          <FormationRow row={describeRow(item, { focused: detailsDrawer.target?.kind === 'repository' && detailsDrawer.target.item.id === item.id, canAct: !gitBusy })} editing={editingId === item.id} active={activeRow?.id === item.id} rowIndex={index + 2} handlers={rowHandlers(item)} />
        {/snippet}
      </VirtualList>
    {/key}
    {#if items.length}<Pager total={items.length} bind:page={store.page} bind:size={app.ws.pageSize} {density} ondensity={value => (app.ws.density = value)} />{/if}
  </div>
  <RepositoryBulk bind:this={bulk} {selected} busy={gitBusy} extend={offer ? { count: offer, run: () => { for (const item of items) item.on = true; } } : null} />
</div>

{#if menu}
  <RowMenu item={menu.item} x={menu.x} y={menu.y} opener={menu.opener} onclose={() => (menu = null)} onrename={id => (editingId = id)} onremove={id => removed(id, menuNeighbour)} onaddset={item => (addTo = { item, anchor: menu?.opener ?? document.body })} />
{/if}

{#if addTo}<AddToSetMenu items={[addTo.item]} anchor={addTo.anchor} onclose={() => (addTo = null)} />{/if}
