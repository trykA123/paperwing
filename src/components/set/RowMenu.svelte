<script lang="ts">
  import { onMount } from 'svelte';
  import type { SetItem } from '../../lib/api';
  import { commands, execute } from '../../lib/commands';
  import { needsClone, openHistory } from '../../lib/row-actions';
  import { stashFlow } from '../../lib/stash-flow.svelte';
  import { app } from '../../lib/state.svelte';
  import { disabledReason, type MenuFacts, type Need } from '../../lib/menu-reason';
  import Icon from '../Icon.svelte';

  let { item, x, y, opener, onclose, onrename, onremove }: {
    item: SetItem; x: number; y: number; opener: HTMLElement | null; onclose: () => void; onrename: (id: string) => void; onremove: (id: string) => void;
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
  const cloned = $derived(!!app.local[app.dest(item)]?.repo);
  const facts = $derived.by((): MenuFacts => {
    const local = app.local[app.dest(item)];
    return {
      ready: app.ready, cloned, inPlace: !!item.path, idle: !gitBusy, preparing: app.clonePreparing,
      behind: local?.behind ?? 0, ahead: local?.ahead ?? 0, dirty: local?.dirty ?? 0, onRef: app.onRef(item),
    };
  });
  const stashWhy = $derived(disabledReason(['cloned', 'dirty'], facts));
  const switchWhy = $derived(disabledReason(['managed', 'cloned', 'offRef'], facts) ?? (item.ref.type === 'branch' ? null : 'Only a branch can be switched to with a stash'));
  const why = (needs: Need[], fallback?: string | null) => disabledReason(needs, facts) ?? fallback ?? 'Not available right now';
  let menu: HTMLDivElement;

  const enabled = () => [...menu.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')];
  onMount(() => { enabled()[0]?.focus(); });

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

<div class="row-menu" role="menu" tabindex="-1" bind:this={menu} style:left="{x}px" style:top="{y}px" onkeydown={onKey}>
  <button role="menuitem" disabled={!cloned} title={cloned ? undefined : 'Clone the repository first'} data-tip-side="left" onclick={() => choose(() => openHistory(item, opener), true)}><Icon name="commit" tone="inspect" />History</button>
  <button role="menuitem" onclick={() => choose(() => { app.inspectedId = item.id; app.ws.shell.rightVisible = true; })}><Icon name="folder" tone="inspect" />Show details</button>
  <button role="menuitem" disabled={!!disabledReason(['managed', 'cloned'], facts)} title={disabledReason(['managed', 'cloned'], facts) ?? undefined} data-tip-side="left" onclick={() => choose(() => app.openCompare(item, true))}><Icon name="code" tone="inspect" />Compare</button>
  <hr />
  {#each commands([item]).filter(command => IDS.includes(command.id) && (command.id !== 'clone' || needsClone(item))) as command (command.id)}
    <button role="menuitem" title={command.enabled ? undefined : why(NEEDS[command.id] ?? [], command.reason)} data-tip-side="left" disabled={!command.enabled} onclick={() => choose(() => execute(command), FOCUS_TAKERS.includes(command.id))}><Icon name={command.icon} tone={command.tone} />{LABELS[command.id]}</button>
  {/each}
  <button role="menuitem" disabled={!!stashWhy} title={stashWhy ?? undefined} data-tip-side="left" onclick={() => choose(() => stashFlow.openPush([item]), true)}><Icon name="stash" tone="record" />Stash changes…</button>
  <button role="menuitem" disabled={!!switchWhy} title={switchWhy ?? undefined} data-tip-side="left" onclick={() => choose(() => stashFlow.openSwitch([item]), true)}><Icon name="stash" tone="record" />Switch with stash…</button>
  <hr />
  <button role="menuitem" disabled={!!item.path} title={disabledReason(['managed'], facts) ?? undefined} data-tip-side="left" onclick={() => choose(() => onrename(item.id), true)}>Rename folder</button>
  <button role="menuitem" disabled={!!item.path} title={disabledReason(['managed'], facts) ?? undefined} data-tip-side="left" onclick={() => choose(() => app.duplicateItem(item.id))}><Icon name="copy" />Duplicate into another folder</button>
  <button role="menuitem" onclick={() => { const id = item.id; choose(() => { void app.removeItem(id).then(() => onremove(id)); }, true); }}><Icon name="close" />Remove from set</button>
</div>
