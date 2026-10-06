<script lang="ts">
  import { onMount } from 'svelte';
  import type { SetItem } from '../../lib/api';
  import { commands, execute } from '../../lib/commands';
  import { needsClone, openHistory } from '../../lib/row-actions';
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';

  let { item, x, y, opener, onclose, onrename }: {
    item: SetItem; x: number; y: number; opener: HTMLElement | null; onclose: () => void; onrename: (id: string) => void;
  } = $props();

  const LABELS: Record<string, string> = {
    clone: 'Clone', fetch: 'Fetch', pull: 'Pull (fast-forward)', push: 'Push', switch: 'Switch to the set’s branch',
    commit: 'Commit changes…', 'new-branch': 'New branch…', cleanup: 'Clean up merged branches…', code: 'Open in VS Code',
  };
  const WHY: Record<string, string> = {
    clone: 'Nothing to clone', fetch: 'Clone the repository first', pull: 'Not behind its upstream, or it has local commits to push first', push: 'Nothing to push',
    switch: 'Already on the set’s branch', commit: 'No uncommitted changes', 'new-branch': 'Clone the repository first', cleanup: 'Clone the repository first', code: 'Clone the repository first',
  };
  const IDS = Object.keys(LABELS);
  const FOCUS_TAKERS = ['commit', 'new-branch', 'cleanup', 'code'];
  const gitBusy = $derived(app.running || app.gitBusy);
  const cloned = $derived(!!app.local[app.dest(item)]?.repo);
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
  <button role="menuitem" disabled={gitBusy || !cloned || !!item.path} title={item.path ? 'Not available for folders opened in place yet' : gitBusy ? 'A Git operation is already running' : !cloned ? 'Clone the repository first' : undefined} data-tip-side="left" onclick={() => choose(() => app.openCompare(item, true))}><Icon name="code" tone="inspect" />Compare</button>
  <hr />
  {#each commands([item]).filter(command => IDS.includes(command.id) && (command.id !== 'clone' || needsClone(item))) as command (command.id)}
    <button role="menuitem" title={command.enabled ? undefined : command.reason ?? (gitBusy ? 'A Git operation is already running' : WHY[command.id] ?? 'Not available right now')} data-tip-side="left" disabled={!command.enabled} onclick={() => choose(() => execute(command), FOCUS_TAKERS.includes(command.id))}><Icon name={command.icon} tone={command.tone} />{LABELS[command.id]}</button>
  {/each}
  <hr />
  <button role="menuitem" disabled={!!item.path} title={item.path ? 'Folders opened in place cannot be renamed here' : undefined} data-tip-side="left" onclick={() => choose(() => onrename(item.id), true)}>Rename folder</button>
  <button role="menuitem" disabled={!!item.path} title={item.path ? 'Folders opened in place cannot be duplicated' : undefined} data-tip-side="left" onclick={() => choose(() => app.duplicateItem(item.id))}><Icon name="copy" />Duplicate into another folder</button>
  <button role="menuitem" onclick={() => choose(() => void app.removeItem(item.id))}><Icon name="close" />Remove from set</button>
</div>
