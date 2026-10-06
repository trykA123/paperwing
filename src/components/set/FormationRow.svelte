<script lang="ts" module>
  import type { NextActionKind } from '../../lib/formation';

  export type RowHandlers = {
    toggle: (on: boolean) => void; inspect: () => void; pickRef: (anchor: HTMLElement) => void;
    next: (kind: NextActionKind) => void; menu: (anchor: HTMLElement | { x: number; y: number }) => void;
    rename: (value: string | null) => void;
  };
</script>

<script lang="ts">
  import type { RowModel } from '../../lib/formation-row';
  import Icon, { type IconName, type IconTone } from '../Icon.svelte';
  import SyncRails from './SyncRails.svelte';

  let { row, editing, handlers }: { row: RowModel; editing: boolean; handlers: RowHandlers } = $props();

  const ICONS: Record<NextActionKind, { icon: IconName; tone: IconTone }> = {
    clone: { icon: 'folder', tone: 'sync' }, commit: { icon: 'check', tone: 'record' }, switch: { icon: 'branch', tone: 'branch' },
    pull: { icon: 'download', tone: 'sync' }, push: { icon: 'upload', tone: 'sync' },
  };

  const focus = (el: HTMLInputElement) => { el.focus(); el.select(); };

  function onKey(event: KeyboardEvent) {
    if (event.key !== 'ContextMenu' && !(event.shiftKey && event.key === 'F10')) return;
    event.preventDefault();
    const box = event.currentTarget instanceof HTMLElement ? event.currentTarget.getBoundingClientRect() : null;
    if (box) handlers.menu({ x: box.left, y: box.bottom });
  }

  function onRowClick(event: MouseEvent) {
    if (!(event.target as Element).closest('button, input, a')) handlers.inspect();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<div class="fm-row" role="group" aria-label="Repository {row.folder}" class:sel={row.selected} class:focus={row.focused} class:busy={!!row.busy}
  onclick={onRowClick} oncontextmenu={event => { event.preventDefault(); handlers.menu({ x: event.clientX, y: event.clientY }); }}>
  <div class="fm-cell fm-check">
    <input type="checkbox" checked={row.selected} onchange={event => handlers.toggle(event.currentTarget.checked)} aria-label="Select {row.folder}" />
  </div>
  <div class="fm-cell fm-repo">
    {#if editing}
      <input class="fname-edit" value={row.folder} use:focus spellcheck="false" aria-label="Folder name" onblur={event => handlers.rename(event.currentTarget.value)}
        onkeydown={event => { if (event.key === 'Enter') event.currentTarget.blur(); else if (event.key === 'Escape') handlers.rename(null); }} />
    {:else}
      <button class="fm-name" title="{row.item.org}/{row.item.name} · show details" onclick={handlers.inspect} onkeydown={onKey}>{row.folder}</button>
    {/if}
    <small class="fm-sub">
      {#if row.problem}<span class="fm-problem" title={row.problem}><Icon name="alert" size={12} tone="err" />{row.problem}</span>
      {:else}<span class="fm-org" title={row.sub}>{row.sub}</span>{#if row.onDisk}<span class="fm-ondisk"><Icon name="check" size={11} tone="ok" />on disk</span>{/if}{/if}
    </small>
  </div>
  <div class="fm-cell fm-branch">
    {#if row.fixed}
      <span class="fm-ref static" title={row.refTitle}><span class="t-{row.refType}"><Icon name={row.refType} /></span><span class="nm">{row.refLabel || 'detached'}</span></span>
    {:else}
      <button class="fm-ref" class:bad={row.refBad} onclick={event => handlers.pickRef(event.currentTarget)} title={row.refTitle}>
        <span class="t-{row.refType}"><Icon name={row.refType} /></span><span class="nm">{row.refLabel}</span><span class="car" aria-hidden="true">▾</span>
      </button>
      {#if row.localNote}<small class="fm-note" title="The folder has a different branch checked out">{row.localNote}</small>{/if}
    {/if}
  </div>
  <div class="fm-cell fm-syncc"><SyncRails view={row.sync} /></div>
  <div class="fm-cell fm-next">
    {#if row.busy}
      <span class="fm-busy" role="status"><span class="spin"></span>{row.busy}…</span>
    {:else if row.next}
      {@const kind = row.next.kind}
      <button class="btn small fm-action" disabled={!row.canAct} title={row.next.title} onclick={() => handlers.next(kind)}>
        <Icon name={ICONS[kind].icon} tone={ICONS[kind].tone} />{row.next.label}
      </button>
    {/if}
  </div>
  <div class="fm-cell fm-more">
    <button class="fm-menu-btn" aria-label="More actions for {row.folder}" aria-haspopup="menu" title="More actions" onclick={event => handlers.menu(event.currentTarget)}><Icon name="more" size={16} /></button>
  </div>
</div>
