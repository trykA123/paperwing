<script lang="ts">
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { isCloned } from '../../lib/formation';
  import { describeRow } from '../../lib/formation-row';
  import type { RepoEntry } from '../../lib/repositories';
  import { applyRef } from '../../lib/ref-apply';
  import { runNextAction } from '../../lib/row-actions';
  import { app } from '../../lib/state.svelte';
  import Icon, { type IconName, type IconTone } from '../Icon.svelte';
  import Popover from '../Popover.svelte';
  import RefPicker from '../RefPicker.svelte';
  import RowMenu from '../set/RowMenu.svelte';
  import SyncRails from '../set/SyncRails.svelte';
  import AddToSetMenu from './AddToSetMenu.svelte';

  let { entry }: { entry: RepoEntry } = $props();

  const store = app.repositories;
  const ICONS: Record<string, { icon: IconName; tone: IconTone }> = {
    clone: { icon: 'folder', tone: 'sync' }, adopt: { icon: 'plus', tone: 'sync' }, commit: { icon: 'check', tone: 'record' }, switch: { icon: 'branch', tone: 'branch' },
    diverged: { icon: 'commit', tone: 'warn' }, pull: { icon: 'download', tone: 'sync' }, push: { icon: 'upload', tone: 'sync' },
  };
  let picker = $state<DOMRect | null>(null);
  let openIn = $state<HTMLElement | null>(null);
  let more = $state<{ x: number; y: number; opener: HTMLElement } | null>(null);
  let addTo = $state<HTMLElement | null>(null);
  const cloned = $derived(isCloned(app.local[app.dest(entry.item)]));
  const busy = $derived(app.running || app.gitBusy || app.clonePreparing);
  const row = $derived(describeRow(entry.item, { focused: false, canAct: !busy }));
  const path = $derived(app.dest(entry.item));
  const web = $derived(entry.url.replace(/\.git$/, ''));

  const narrow = (patch: { hostFilter?: string; org?: string }) => { store.filter({ hostFilter: '', org: '', ...patch }); store.back(); };
  const copy = (text: string) => { void navigator.clipboard.writeText(text); app.toast('Copied', 'success'); };
</script>

<header class="mh rf-head">
  <div class="grow">
    <nav class="rf-crumbs" aria-label="Breadcrumb">
      <button onclick={() => { store.clearFilters(); store.back(); }}>Repositories</button>
      {#if entry.host}<span aria-hidden="true">›</span><button title="Show only {entry.host}" onclick={() => narrow({ hostFilter: entry.host })}>{entry.host}</button>{/if}
      {#if entry.org}<span aria-hidden="true">›</span><button title="Show only {entry.org}" onclick={() => narrow({ hostFilter: entry.host, org: entry.org })}>{entry.org}</button>{/if}
    </nav>
    <h1>{app.folderOf(entry.item)}
      <button class="rf-star" class:on={entry.favorite} aria-pressed={entry.favorite} title={entry.favorite ? 'Remove from favorites' : 'Add to favorites'} aria-label="{entry.favorite ? 'Remove' : 'Add'} {entry.name} {entry.favorite ? 'from' : 'to'} favorites" onclick={() => app.toggleStar(entry.repoId)}><Icon name="star" size={16} /></button>
      <span class="rf-tag" class:remote={!cloned}>{cloned ? (entry.remoteOnly ? 'on disk · no set' : 'cloned') : 'remote only'}</span>
    </h1>
    <div class="rf-meta">
      <span class="rf-meta-i"><Icon name="server" size={12} />{entry.host}</span>
      {#if cloned}
        <span class="mono rf-path" title={path}>{path}</span>
        {#if row.fixed}<span class="fm-ref static"><span class="t-{row.refType}"><Icon name={row.refType} /></span><span class="nm">{row.refLabel || 'detached'}</span></span>
        {:else}
          <button class="fm-ref" class:bad={row.refBad} title={row.refTitle} onclick={event => (picker = event.currentTarget.getBoundingClientRect())}>
            <span class="t-{row.refType}"><Icon name={row.refType} /></span><span class="nm">{row.refLabel}</span><span class="car" aria-hidden="true">▾</span>
          </button>
        {/if}
        <SyncRails view={row.sync} />
      {:else}
        <span class="fm-ref static" title="Default branch on the remote"><span class="t-branch"><Icon name="branch" /></span><span class="nm">{entry.defaultBranch}</span></span>
        <span class="mut">Not cloned. Local sections need a clone.</span>
      {/if}
    </div>
  </div>
  <div class="hbtns">
    {#if row.busy}<span class="fm-busy" role="status"><span class="spin"></span>{row.busy}…</span>
    {:else if row.next}{@const kind = row.next.kind}<button class="btn dark" disabled={busy} title={row.next.title} aria-label={row.next.aria} onclick={() => runNextAction(entry.item, kind)}><Icon name={ICONS[kind].icon} />{row.next.label}</button>{/if}
    {#if cloned}<button class="btn" disabled={busy || !!entry.item.path} title="git fetch --prune" onclick={() => app.startClone([entry.item], 'fetch')}><Icon name="refresh" />Fetch</button>{/if}
    <button class="btn" aria-haspopup="dialog" aria-expanded={!!openIn} onclick={event => (openIn = openIn ? null : event.currentTarget)}><Icon name="external" />Open in<span class="car" aria-hidden="true">▾</span></button>
    <button class="btn icon-only" aria-haspopup="menu" aria-label="More actions" title="More actions" onclick={event => { const box = event.currentTarget.getBoundingClientRect(); more = { x: Math.max(8, box.right - 290), y: box.bottom + 4, opener: event.currentTarget }; }}><Icon name="more" /></button>
  </div>
</header>

{#if picker}<RefPicker items={[entry.item]} anchor={picker} onclose={() => (picker = null)} onpick={ref => { picker = null; applyRef([entry.item], ref); }} />{/if}
{#if openIn}
  <Popover anchor={openIn} label="Open in" onclose={() => (openIn = null)} width={240}>
    <div class="menu-list">
      <button class="menu-item" onclick={() => { openIn = null; void openUrl(web); }}><Icon name="remote" tone="inspect" /><span class="lbl">Browser on {entry.host}</span></button>
      {#if cloned}
        <button class="menu-item" onclick={() => { openIn = null; app.openVscode(path); }}><Icon name="code" /><span class="lbl">VS Code</span></button>
        <button class="menu-item" onclick={() => { openIn = null; copy(path); }}><Icon name="copy" /><span class="lbl">Copy folder path</span></button>
      {/if}
      <button class="menu-item" onclick={() => { openIn = null; copy(entry.url); }}><Icon name="copy" /><span class="lbl">Copy clone URL</span></button>
    </div>
  </Popover>
{/if}
{#if more}<RowMenu page item={entry.item} x={more.x} y={more.y} opener={more.opener} onclose={() => (more = null)} onrename={() => {}} onremove={() => store.back()} onaddset={() => (addTo = more?.opener ?? document.body)} />{/if}
{#if addTo}<AddToSetMenu items={[entry.item]} anchor={addTo} onclose={() => (addTo = null)} />{/if}
