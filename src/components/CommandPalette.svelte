<script lang="ts">
  import { onMount } from 'svelte';
  import { app, matches } from '../lib/state.svelte';
  import { commands, execute } from '../lib/commands';
  import Icon from './Icon.svelte';
  import { dialogOut } from '../lib/motion';

  let dialog: HTMLDialogElement;
  let input: HTMLInputElement;
  let query = $state('');
  let selected = $state(0);
  const available = $derived(commands().filter(command => matches(command.label, query) && (command.enabled || !!command.reason)));
  const current = $derived(Math.min(selected, Math.max(0, available.length - 1)));

  onMount(() => {
    const previous = document.activeElement as HTMLElement | null;
    dialog.showModal();
    input.focus();
    return () => previous?.focus();
  });

  function key(event: KeyboardEvent) {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const length = available.length;
      selected = length ? (current + (event.key === 'ArrowDown' ? 1 : length - 1)) % length : 0;
      dialog.querySelectorAll('.palette-command')[selected]?.scrollIntoView({ block: 'nearest' });
    } else if (event.key === 'Enter') {
      event.preventDefault();
      if (available[current]) execute(available[current]);
    }
  }
</script>

<dialog class="palette" bind:this={dialog} out:dialogOut|global oncancel={() => (app.paletteOpen = false)} aria-label="Command palette">
  <div class="palette-search"><Icon name="search" size={18} /><input bind:this={input} bind:value={query} oninput={() => (selected = 0)} onkeydown={key} aria-label="Find a command" placeholder="Search commands..." />
    <button class="icon" title="Close command palette" aria-label="Close command palette" onclick={() => (app.paletteOpen = false)}><Icon name="close" /></button></div>
  <div class="palette-results">
    {#each available as command, index (command.id)}
      <button class="palette-command" class:on={index === current} disabled={!command.enabled} title={command.reason ?? command.label} onclick={() => execute(command)}><Icon name={command.icon} tone={command.tone} /><span>{command.label}{#if !command.enabled && command.reason}<small class="faint"> · {command.reason}</small>{/if}</span></button>
    {:else}<div class="empty">No available commands match.</div>{/each}
  </div>
</dialog>