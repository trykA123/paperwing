<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../lib/state.svelte';
  import { commands, commandGroup, commandShortcut, execute, GROUPS, isRisky, type Command } from '../lib/commands';
  import { arrange, defaultIndex, nameBonus, stepIndex } from '../lib/palette';
  import { fuzzy } from '../lib/fuzzy';
  import Icon from './Icon.svelte';
  import { dialogOut } from '../lib/motion';
  import { paletteReturn } from '../lib/focus-trap';

  const REPO_LIMIT = 5;
  const repoName = (command: Command) => app.set.items.find(item => `repo:${item.id}` === command.id)?.name;
  let dialog: HTMLDialogElement;
  let input: HTMLInputElement;
  let query = $state('');
  let picked = $state<number | null>(null);

  const repoCommands = $derived.by((): Command[] => {
    if (!query.trim()) return [];
    return app.set.items
      .flatMap(item => { const found = fuzzy(query, item.name) ?? fuzzy(query, `${item.org}/${item.name}`); return found ? [{ item, score: found.score + nameBonus(item.name, query) }] : []; })
      .sort((a, b) => b.score - a.score).slice(0, REPO_LIMIT)
      .map(({ item }) => ({
        id: `repo:${item.id}`, label: `Open details: ${item.org}/${item.name}`, icon: 'folder' as const, tone: 'inspect' as const, enabled: true,
        run: () => { app.inspectedId = item.id; app.ws.shell.rightVisible = true; },
      }));
  });
  const available = $derived.by(() => {
    const list = [...commands().filter(command => command.enabled || !!command.reason), ...repoCommands];
    return arrange(list, query, [...GROUPS, 'Repositories'], command => (command.id.startsWith('repo:') ? 'Repositories' : commandGroup(command)), {
      text: command => repoName(command) ?? command.label,
      bonus: (command, q) => { const name = repoName(command); return name ? nameBonus(name, q) : 0; },
    });
  });
  const current = $derived(picked !== null && picked < available.length ? picked : defaultIndex(available, isRisky));
  const grouped = $derived(!query.trim());

  onMount(() => {
    const previous = document.activeElement as HTMLElement | null;
    paletteReturn.element = previous;
    dialog.showModal();
    input.focus();
    return () => previous?.focus();
  });

  function key(event: KeyboardEvent) {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const direction = event.key === 'ArrowDown' ? 1 : -1;
      picked = available.length ? stepIndex(available, current < 0 ? (direction === 1 ? -1 : 0) : current, direction) : null;
      dialog.querySelectorAll('.palette-command')[picked ?? 0]?.scrollIntoView({ block: 'nearest' });
    } else if (event.key === 'Enter') {
      event.preventDefault();
      if (available[current]) execute(available[current].command);
    }
  }
</script>

<dialog class="palette" bind:this={dialog} out:dialogOut|global oncancel={() => (app.paletteOpen = false)} aria-label="Command palette">
  <div class="palette-search"><Icon name="search" size={18} /><input bind:this={input} bind:value={query} oninput={() => (picked = null)} onkeydown={key} aria-label="Find a command or repository" placeholder="Search commands and repositories" />
    <button class="icon" title="Close command palette" aria-label="Close command palette" onclick={() => (app.paletteOpen = false)}><Icon name="close" /></button></div>
  <div class="palette-results">
    {#each available as { command, group }, index (command.id)}
      {#if grouped && group !== available[index - 1]?.group}<div class="palette-group">{group}</div>{/if}
      {@const shortcut = commandShortcut(command)}
      <button class="palette-command" class:on={index === current} class:off={!command.enabled} aria-disabled={!command.enabled} onclick={() => execute(command)}>
        <Icon name={command.icon} tone={command.tone} />
        <span class="pc-text">{command.label}{#if !command.enabled && command.reason}<small class="pc-reason">{command.reason}</small>{/if}</span>
        {#if shortcut}<span class="pc-keys">{#each shortcut.split(' ') as part}<kbd>{part}</kbd>{/each}</span>{/if}
      </button>
    {:else}<div class="empty">No command matches "{query.trim()}". Try a repository name.</div>{/each}
  </div>
</dialog>
