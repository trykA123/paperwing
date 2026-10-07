<script lang="ts" module>
  import type { NextActionKind } from '../../lib/formation';

  export type RowHandlers = {
    toggle: (on: boolean, range: boolean) => void; activate: () => void; inspect: () => void; pickRef: (anchor: HTMLElement) => void;
    next: (kind: NextActionKind) => void; menu: (anchor: HTMLElement | { x: number; y: number; opener?: HTMLElement }) => void;
    rename: (value: string | null) => void; retryStatus: () => void;
  };
</script>

<script lang="ts">
  import type { RowModel } from '../../lib/formation-row';
  import Icon, { type IconName, type IconTone } from '../Icon.svelte';
  import PullCell from '../pulls/PullCell.svelte';
  import SyncRails from './SyncRails.svelte';

  let { row, editing, active, rowIndex, handlers }: { row: RowModel; editing: boolean; active: boolean; rowIndex: number; handlers: RowHandlers } = $props();

  const stop = $derived(active ? 0 : -1);

  const ICONS: Record<NextActionKind, { icon: IconName; tone: IconTone }> = {
    clone: { icon: 'folder', tone: 'sync' }, commit: { icon: 'check', tone: 'record' }, switch: { icon: 'branch', tone: 'branch' },
    diverged: { icon: 'commit', tone: 'warn' }, pull: { icon: 'download', tone: 'sync' }, push: { icon: 'upload', tone: 'sync' },
  };

  const focus = (el: HTMLInputElement) => { el.focus(); el.select(); };

  function onKey(event: KeyboardEvent) {
    if (event.key !== 'ContextMenu' && !(event.shiftKey && event.key === 'F10')) return;
    event.preventDefault();
    const opener = event.currentTarget instanceof HTMLElement ? event.currentTarget : null;
    const box = opener?.getBoundingClientRect();
    if (opener && box) handlers.menu({ x: box.left, y: box.bottom, opener });
  }

  function onRowClick(event: MouseEvent) {
    if (!(event.target as Element).closest('button, input, a, label')) handlers.inspect();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<div class="fm-row" role="row" aria-rowindex={rowIndex} tabindex={stop} data-id={row.item.id} aria-selected={row.selected} class:sel={row.selected} class:focus={row.focused} class:busy={!!row.busy}
  onclick={onRowClick} onfocusin={handlers.activate} onkeydown={onKey} oncontextmenu={event => { event.preventDefault(); handlers.menu({ x: event.clientX, y: event.clientY }); }}>
  <div class="fm-cell fm-check" role="gridcell">
    <label class="fm-hit"><input type="checkbox" tabindex={stop} checked={row.selected} onclick={event => handlers.toggle(event.currentTarget.checked, event.shiftKey)} aria-label="Select {row.folder}" /></label>
  </div>
  <div class="fm-cell fm-repo" role="gridcell">
    {#if editing}
      <input class="fname-edit" value={row.folder} use:focus spellcheck="false" aria-label="Folder name" onblur={event => handlers.rename(event.currentTarget.value)}
        onkeydown={event => { if (event.key === 'Enter') event.currentTarget.blur(); else if (event.key === 'Escape') handlers.rename(null); }} />
    {:else}
      <button class="fm-name" title="{row.item.org}/{row.item.name} · show details" tabindex={stop} onclick={handlers.inspect}>{row.folder}</button>
    {/if}
    <small class="fm-sub">
      {#if row.problem}<span class="fm-problem" title={row.problem}><Icon name="alert" size={12} tone="err" />{row.problem}</span>
      {:else}<span class="fm-org" title={row.sub}>{row.sub}</span>{#if row.onDisk}<span class="fm-ondisk"><Icon name="check" size={11} tone="ok" />on disk</span>{/if}{/if}
    </small>
  </div>
  <div class="fm-cell fm-branch" role="gridcell">
    {#if row.fixed}
      <span class="fm-ref static" title={row.refTitle}><span class="t-{row.refType}"><Icon name={row.refType} /></span><span class="nm">{row.refLabel || 'detached'}</span></span>
    {:else}
      <button class="fm-ref" tabindex={stop} class:bad={row.refBad} onclick={event => handlers.pickRef(event.currentTarget)} title={row.refTitle}>
        <span class="t-{row.refType}"><Icon name={row.refType} /></span><span class="nm">{row.refLabel}</span><span class="car" aria-hidden="true">▾</span>
      </button>
      {#if row.localNote}
        <small class="fm-note" title="The folder has a different branch checked out">{row.localNote}</small>
        <span class="fm-note-icon" role="img" aria-label="{row.localNote}: the folder has a different branch checked out" title="{row.localNote}: the folder has a different branch checked out"><Icon name="alert" size={14} tone="warn" /></span>
      {/if}
    {/if}
  </div>
  <div class="fm-cell fm-pull" role="gridcell"><PullCell target={row.pull} /></div>
  <div class="fm-cell fm-syncc" role="gridcell"><SyncRails view={row.sync} /></div>
  <div class="fm-cell fm-next" role="gridcell">
    {#if row.busy}
      <span class="fm-busy" role="status"><span class="spin"></span>{row.busy}…</span>
    {:else if row.next}
      {@const kind = row.next.kind}
      <button class="btn small fm-action" tabindex={stop} disabled={!row.canAct} title={row.next.title} aria-label={row.next.aria} onclick={() => handlers.next(kind)}>
        <Icon name={ICONS[kind].icon} tone={ICONS[kind].tone} /><span class="fm-action-label">{row.next.label}</span>
      </button>
    {:else if row.sync.kind === 'unavailable'}
      <button class="btn small fm-action" tabindex={stop} title="Read the status of this folder again" onclick={handlers.retryStatus}><Icon name="refresh" /><span class="fm-action-label">Retry</span></button>
    {/if}
  </div>
  <div class="fm-cell fm-more" role="gridcell">
    <button class="fm-menu-btn" tabindex={stop} aria-label="More actions for {row.folder}" aria-haspopup="menu" title="More actions" onclick={event => handlers.menu(event.currentTarget)}><Icon name="more" size={16} /></button>
  </div>
</div>
