<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import type { SetItem } from '../../lib/api';
  import { commands, execute } from '../../lib/commands';
  import { needsClone, openHistory, runNextAction } from '../../lib/row-actions';
  import { pullFlow, pullKey } from '../../lib/pull-flow.svelte';
  import { openPull, pulls } from '../../lib/pulls.svelte';
  import { detailsDrawer } from '../../lib/details-drawer.svelte';
  import { isRemoteItem } from '../../lib/repositories';
  import { stashFlow } from '../../lib/stash-flow.svelte';
  import { tagFlow } from '../../lib/tag-flow.svelte';
  import { app } from '../../lib/state.svelte';
  import { disabledReason, type MenuFacts, type Need } from '../../lib/menu-reason';
  import Icon from '../Icon.svelte';

  let { item, x, y, opener, onclose, onrename, onremove, onaddset, page = false }: {
    page?: boolean; item: SetItem; x: number; y: number; opener: HTMLElement | null; onclose: () => void; onrename: (id: string) => void; onremove: (id: string) => void; onaddset: (item: SetItem) => void;
  } = $props();

  const LABELS: Record<string, string> = {
    clone: 'Clone', fetch: 'Fetch', pull: 'Pull (fast-forward)', push: 'Push', switch: 'Switch to the set’s branch',
    commit: 'Commit changes…', 'new-branch': 'New branch…', cleanup: 'Clean up merged branches…', code: 'Open in VS Code',
  };
  const NEEDS: Record<string, Need[]> = {
    clone: [], fetch: ['managed', 'cloned'], pull: ['managed', 'cloned', 'behind'], push: ['cloned', 'unpushed'], switch: ['managed', 'cloned', 'offRef'],
    commit: ['cloned', 'dirty'], 'new-branch': ['cloned'], cleanup: ['cloned'], code: ['cloned'],
  };
  const IDS = Object.keys(LABELS);
  const FOCUS_TAKERS = ['commit', 'new-branch', 'cleanup', 'code'];
  const gitBusy = $derived(app.running || app.gitBusy);
  const remote = $derived(app.repositories.isUncloned(item));
  const virtual = $derived(isRemoteItem(item));
  const starred = $derived(app.ws.stars.includes(item.repoId));
  const cloned = $derived(!!app.local[app.dest(item)]?.repo);
  const facts = $derived.by((): MenuFacts => {
    const local = app.local[app.dest(item)];
    return {
      ready: app.ready, cloned, inPlace: !!item.path, idle: !gitBusy, preparing: app.clonePreparing,
      behind: local?.behind ?? 0, ahead: local?.ahead ?? 0, dirty: local?.dirty ?? 0, onRef: app.onRef(item), hasBranch: !!local?.branch,
    };
  });
  const stashWhy = $derived(disabledReason(['cloned', 'dirty'], facts));
  const switchWhy = $derived(disabledReason(['managed', 'cloned', 'offRef'], facts) ?? (item.ref.type === 'branch' ? null : 'Only a branch can be switched to with a stash'));
  const tagWhy = $derived(disabledReason(['cloned'], facts));
  const known = $derived.by(() => { const key = pullKey(item); const entry = key ? pulls.entry(key) : undefined; return entry?.status === 'ready' ? entry.pull : null; });
  const live = $derived(known && (known.state === 'open' || known.state === 'draft') ? known : null);
  const pullWhy = $derived(disabledReason(['cloned', 'branch'], facts) ?? (live ? `Pull request #${live.number} is already open` : null));
  const why = (needs: Need[], fallback?: string | null) => disabledReason(needs, facts) ?? fallback ?? 'Not available right now';
  let menu: HTMLDivElement;

  const enabled = () => [...menu.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')];
  let top = $state(untrack(() => y));
  onMount(() => {
    top = Math.max(8, Math.min(y, innerHeight - menu.offsetHeight - 8));
    enabled()[0]?.focus();
  });

  function close() { const origin = opener; onclose(); origin?.focus(); }
  /** Actions that open a dialog, drawer or input take focus themselves; the rest hand it back to the opener. */
  function choose(run: () => void, takesFocus = false) {
    const origin = opener;
    run();
    onclose();
    if (!takesFocus) origin?.focus();
  }

  function onKey(event: KeyboardEvent) {
    if (event.key === 'Escape' || event.key === 'Tab') { event.preventDefault(); close(); return; }
    const step = event.key === 'ArrowDown' ? 1 : event.key === 'ArrowUp' ? -1 : 0;
    if (!step) return;
    event.preventDefault();
    const list = enabled();
    const at = list.indexOf(document.activeElement as HTMLButtonElement);
    list[(at + step + list.length) % list.length]?.focus();
  }
</script>

<svelte:window onpointerdown={event => { if (!(event.target as Element).closest('.row-menu')) onclose(); }} />

<div class="row-menu" role="menu" tabindex="-1" bind:this={menu} style:left="{x}px" style:top="{top}px" onkeydown={onKey}>
  <button role="menuitem" onclick={() => choose(() => app.toggleStar(item.repoId))}><Icon name="star" tone="warn" />{starred ? 'Remove from favorites' : 'Add to favorites'}</button>
  <button role="menuitem" onclick={() => choose(() => onaddset(item), true)}><Icon name="folder" tone="folder" />Add to set…</button>
  <hr />
{#if remote}
  <button role="menuitem" onclick={() => choose(() => runNextAction(item, 'clone'))}><Icon name="folder" tone="sync" />Clone</button>
{:else}
  <button role="menuitem" disabled={!cloned} title={cloned ? undefined : 'Clone the repository first'} data-tip-side="left" onclick={() => choose(() => openHistory(item, opener), true)}><Icon name="commit" tone="inspect" />History</button>
{#if !page}  <button role="menuitem" onclick={() => choose(() => detailsDrawer.open({ kind: 'repository', item }, opener), true)}><Icon name="folder" tone="inspect" />Quick look</button>{/if}
  <button role="menuitem" onclick={() => choose(() => app.repositories.openRepository(item.repoId), true)}><Icon name="repo" tone="inspect" />Open repository page</button>
  <button role="menuitem" disabled={!!disabledReason(['managed', 'cloned'], facts)} title={disabledReason(['managed', 'cloned'], facts) ?? undefined} data-tip-side="left" onclick={() => choose(() => app.openCompare(item, true))}><Icon name="code" tone="inspect" />Compare</button>
  <hr />
  {#each commands([item]).filter(command => IDS.includes(command.id) && (command.id !== 'clone' || needsClone(item))) as command (command.id)}
    <button role="menuitem" title={command.enabled ? undefined : why(NEEDS[command.id] ?? [], command.reason)} data-tip-side="left" disabled={!command.enabled} onclick={() => choose(() => execute(command), FOCUS_TAKERS.includes(command.id))}><Icon name={command.icon} tone={command.tone} />{LABELS[command.id]}</button>
  {/each}
  <button role="menuitem" disabled={!!stashWhy} title={stashWhy ?? undefined} data-tip-side="left" onclick={() => choose(() => stashFlow.openPush([item], opener), true)}><Icon name="stash" tone="record" />Stash changes…</button>
  <button role="menuitem" disabled={!!switchWhy} title={switchWhy ?? undefined} data-tip-side="left" onclick={() => choose(() => stashFlow.openSwitch([item], opener), true)}><Icon name="stash" tone="record" />Switch with stash…</button>
  <button role="menuitem" disabled={!!tagWhy} title={tagWhy ?? undefined} data-tip-side="left" onclick={() => choose(() => tagFlow.openCreate([item], opener), true)}><Icon name="tag" tone="tag" />New tag…</button>
  <button role="menuitem" disabled={!!tagWhy} title={tagWhy ?? undefined} data-tip-side="left" onclick={() => choose(() => tagFlow.openDelete([item], opener), true)}><Icon name="tag" tone="tag" />Delete tag…</button>
  <button role="menuitem" disabled={!!pullWhy} title={pullWhy ?? undefined} data-tip-side="left" onclick={() => choose(() => pullFlow.openFor(item, opener), true)}><Icon name="branch" tone="sync" />Open pull request…</button>
  {#if live}<button role="menuitem" data-tip-side="left" onclick={() => choose(() => void openPull(live.url))}><Icon name="remote" tone="inspect" />View pull request #{live.number}</button>{/if}
  <hr />
  {#if !page}
  <button role="menuitem" disabled={!!item.path} title={disabledReason(['managed'], facts) ?? undefined} data-tip-side="left" onclick={() => choose(() => onrename(item.id), true)}>Rename folder</button>
  <button role="menuitem" disabled={!!item.path} title={disabledReason(['managed'], facts) ?? undefined} data-tip-side="left" onclick={() => choose(() => app.duplicateItem(item.id))}><Icon name="copy" />Duplicate into another folder</button>
  {/if}
  {#if !virtual}<button role="menuitem" onclick={() => { const id = item.id; choose(() => { void app.removeItem(id).then(() => onremove(id)); }, true); }}><Icon name="close" />Remove from set</button>{/if}
{/if}
</div>
