<script lang="ts">
  import { untrack } from 'svelte';
  import { app } from '../lib/state.svelte';
  import type { Ref, SetItem } from '../lib/api';
  import { bulkTargets, filterCounts, matchesFilter, FILTERS, type FormationFilter } from '../lib/formation';
  import { describeRow } from '../lib/formation-row';
  import { pushTarget, rowFacts, runNextAction } from '../lib/row-actions';
  import VirtualList from './VirtualList.svelte';
  import Pager from './Pager.svelte';
  import RefPicker from './RefPicker.svelte';
  import Icon from './Icon.svelte';
  import EmptyState from './EmptyState.svelte';
  import SetHeader from './set/SetHeader.svelte';
  import FilterChips from './set/FilterChips.svelte';
  import FormationRow from './set/FormationRow.svelte';
  import BulkBar from './set/BulkBar.svelte';
  import RowMenu from './set/RowMenu.svelte';

  const tab = untrack(() => app.activeTab);
  const isFilter = (value: string | undefined): value is FormationFilter => FILTERS.some(filter => filter.id === value);
  let page = $state(tab?.page ?? 0);
  let filter = $state<FormationFilter>(isFilter(tab?.filter) ? tab.filter : 'all');
  let editingId = $state<string | null>(null);
  let checking = $state(false);
  let picker = $state<{ items: SetItem[]; anchor: DOMRect } | null>(null);
  let menu = $state<{ item: SetItem; x: number; y: number; opener: HTMLElement | null } | null>(null);
  $effect(() => { if (tab) { tab.page = page; tab.filter = filter; } });

  const set = $derived(app.set);
  const items = $derived(set.items);
  const counts = $derived(filterCounts(items.map(item => app.local[app.dest(item)])));
  const shown = $derived(filter === 'all' ? items : items.filter(item => matchesFilter(filter, app.local[app.dest(item)])));
  const size = $derived(app.ws.pageSize);
  const pages = $derived(size === 'all' ? 1 : Math.max(1, Math.ceil(shown.length / size)));
  const cur = $derived(Math.min(page, pages - 1));
  const rows = $derived(size === 'all' ? shown : shown.slice(cur * size, cur * size + size));
  const selected = $derived(app.selected);
  const allOn = $derived(shown.length > 0 && shown.every(item => item.on));
  const targets = $derived(bulkTargets(selected, rowFacts));
  const dirty = $derived(targets.cloned.filter(item => (app.local[app.dest(item)]?.dirty ?? 0) > 0));
  const gitBusy = $derived(app.running || app.gitBusy || app.clonePreparing);

  $effect(() => {
    app.ws.activeSet;
    picker = null;
    menu = null;
  });

  // A freshly duplicated row opens straight into folder rename, on whichever page it landed.
  $effect(() => {
    const id = app.renameItemId;
    if (!id) return;
    app.renameItemId = null;
    const k = untrack(() => shown.findIndex(item => item.id === id));
    if (k >= 0 && size !== 'all') page = Math.floor(k / size);
    editingId = id;
  });

  function setFilter(next: FormationFilter) {
    filter = next;
    page = 0;
  }

  function pick(ref: Ref) {
    const targets = picker!.items;
    picker = null;
    if (targets.length === 1) return app.setRef(targets[0], ref);
    const ok = targets.filter(i => {
      const r = app.refs[i.url];
      return !!r && (ref.type === 'branch' ? r.branches : r.tags).includes(ref.name);
    });
    ok.forEach(i => app.setRef(i, ref));
    const miss = targets.filter(i => !ok.includes(i)).map(i => i.name);
    const tail = miss.length ? ` (not in ${miss.slice(0, 4).join(', ')}${miss.length > 4 ? ` +${miss.length - 4} more` : ''})` : '';
    app.toast(`${app.refLabel({ ...targets[0], ref })} applied to ${ok.length} of ${targets.length} repos${tail}`, miss.length ? 'warn' : 'success');
  }

  async function checkRefs() {
    checking = true;
    await app.ensureRefs(selected.filter(item => !item.path).map(item => item.url), true);
    checking = false;
  }

  function openMenu(item: SetItem, anchor: HTMLElement | { x: number; y: number; opener?: HTMLElement }) {
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

  const rowHandlers = (item: SetItem) => ({
    toggle: (on: boolean) => { item.on = on; },
    inspect: () => { app.inspectedId = item.id; },
    pickRef: (anchor: HTMLElement) => { picker = { items: [item], anchor: anchor.getBoundingClientRect() }; },
    next: (kind: Parameters<typeof runNextAction>[1]) => runNextAction(item, kind),
    menu: (anchor: HTMLElement | { x: number; y: number; opener?: HTMLElement }) => openMenu(item, anchor),
    rename: (value: string | null) => rename(item, value),
  });

  const bulk = {
    fetch: () => app.startClone(targets.fetchable, 'fetch'),
    pull: () => app.startClone(targets.behind, 'pull'),
    push: () => app.pushRepos(targets.pushable.map(pushTarget)),
    switch: () => app.startClone(targets.offRef, 'switch'),
    ref: (anchor: HTMLElement) => {
      const remote = selected.filter(item => !item.path);
      if (remote.length) picker = { items: remote, anchor: anchor.getBoundingClientRect() };
    },
    check: checkRefs,
    branch: () => app.openBranchDialog(targets.cloned),
    commit: () => { if (dirty[0]) app.openGitDialog('commit', dirty[0]); },
    clear: () => app.setAllOn(false),
  };
</script>

<SetHeader />

<FilterChips {counts} value={filter} onchange={setFilter} />

<div class="fm-wrap">
  <div class="card fill repository-table fm-table" class:running={app.running || app.clonePreparing}>
    {#key `${set.id}|${filter}|${cur}|${size}`}
      <VirtualList items={rows} rowHeight={56} key={i => i.id}>
        {#snippet header()}
          <div class="fm-row fm-head">
            <div class="fm-cell fm-check"><input type="checkbox" checked={allOn} indeterminate={!allOn && shown.some(item => item.on)}
              onchange={e => { for (const item of shown) item.on = e.currentTarget.checked; }} aria-label="Select all shown repositories" /></div>
            <div class="fm-cell">Repository</div><div class="fm-cell">Branch</div><div class="fm-cell">Sync</div><div class="fm-cell">Next action</div><div class="fm-cell"></div>
          </div>
        {/snippet}
        {#snippet empty()}
          {#if items.length}
            <EmptyState icon="folder" title="Nothing matches this filter" hint="No repository in this set is {FILTERS.find(entry => entry.id === filter)?.label.toLowerCase()} right now.">
              <button class="btn" onclick={() => setFilter('all')}>Show all</button>
            </EmptyState>
          {:else if app.isTemporary}
            <EmptyState icon="folder" title={app.temporary.find(set.id)?.scanning ? 'Scanning for repositories' : 'No repositories found'} hint="Repositories appear here as the scan finds them." />
          {:else}
            <EmptyState icon="folder" title="This set is empty" hint="Add repositories from an organization, the search, or your favorites.">
              <button class="btn dark" onclick={() => app.goAddRepos()}><Icon name="plus" /> Add repositories</button>
              <span class="mut">or press <kbd>Ctrl K</kbd> to search commands</span>
            </EmptyState>
          {/if}
        {/snippet}
        {#snippet row(item: SetItem)}
          <FormationRow row={describeRow(item, { focused: app.detailItem?.id === item.id, canAct: !gitBusy })} editing={editingId === item.id} handlers={rowHandlers(item)} />
        {/snippet}
      </VirtualList>
    {/key}
    {#if shown.length}<Pager total={shown.length} bind:page bind:size={app.ws.pageSize} />{/if}
  </div>
  <BulkBar count={selected.length} refEligible={selected.filter(item => !item.path).length} {targets} dirty={dirty.length} busy={gitBusy} {checking} handlers={bulk} />
</div>

{#if picker}
  <RefPicker items={picker.items} anchor={picker.anchor} onclose={() => (picker = null)} onpick={pick} />
{/if}

{#if menu}
  <RowMenu item={menu.item} x={menu.x} y={menu.y} opener={menu.opener} onclose={() => (menu = null)} onrename={id => (editingId = id)} />
{/if}
