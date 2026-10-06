<script lang="ts" module>
  import type { IconName, IconTone } from './Icon.svelte';
  export type RefGroup = { label: string; icon: IconName; tone: IconTone; names: string[]; labels?: Record<string, string>; notes?: Record<string, string> };
</script>

<script lang="ts">
  import { rank, segments } from '../lib/fuzzy';
  import Icon from './Icon.svelte';

  let { value = $bindable(''), groups, placeholder = 'HEAD', disabled = false, id, label }: {
    value?: string; groups: RefGroup[]; placeholder?: string; disabled?: boolean; id: string; label?: string;
  } = $props();

  let input: HTMLInputElement;
  let open = $state(false);
  let active = $state(-1);
  let typed = $state(false);
  let box = $state<{ left: number; width: number; top: number; bottom: number } | null>(null);

  // The list only filters once the user types, so an existing value never hides the other refs.
  const displayValue = $derived(typed ? value : groups.find(group => group.names.includes(value))?.labels?.[value] ?? value);
  const query = $derived(typed ? value.trim() : '');
  type Row = { kind: 'head'; label: string } | { kind: 'ref'; name: string; label: string; positions: number[]; note?: string; icon: IconName; tone: IconTone; index: number };
  const rows = $derived.by(() => {
    const out: Row[] = [];
    let index = 0;
    for (const group of groups) {
      const found = rank(query, group.names, name => group.labels?.[name] ?? name);
      if (!found.length) continue;
      out.push({ kind: 'head', label: group.label });
      for (const { item: name, positions } of found) out.push({ kind: 'ref', name, label: group.labels?.[name] ?? name, positions, note: group.notes?.[name], icon: group.icon, tone: group.tone, index: index++ });
    }
    return out;
  });
  const options = $derived(rows.filter((row): row is Extract<Row, { kind: 'ref' }> => row.kind === 'ref'));
  const room = $derived(box ? innerHeight - box.bottom : 0);
  const upward = $derived(!!box && room < 200 && box.top > room);

  function show() {
    if (disabled) return;
    const rect = input.getBoundingClientRect();
    box = { left: rect.left, width: rect.width, top: rect.top, bottom: rect.bottom };
    open = true;
    if (!typed) {
      active = options.findIndex(option => option.name === value);
      if (active >= 0) queueMicrotask(() => document.querySelector(`#${id}-list [data-index="${active}"]`)?.scrollIntoView({ block: 'nearest' }));
    }
  }

  function choose(name: string) {
    value = name === 'HEAD' ? '' : name;
    typed = false;
    open = false;
    active = -1;
    input.focus();
  }

  function move(step: number) {
    if (!open) show();
    if (!options.length) return;
    active = (active + step + options.length) % options.length;
    queueMicrotask(() => document.querySelector(`#${id}-list [data-index="${active}"]`)?.scrollIntoView({ block: 'nearest' }));
  }

  function key(event: KeyboardEvent) {
    if (event.key === 'ArrowDown') { event.preventDefault(); move(active < 0 ? 0 : 1); }
    else if (event.key === 'ArrowUp') { event.preventDefault(); move(-1); }
    else if (event.key === 'Enter' && open && active >= 0 && options[active]) { event.preventDefault(); choose(options[active].name); }
    else if (event.key === 'Escape' && open) { event.preventDefault(); event.stopPropagation(); open = false; }
  }
</script>

<div class="refselect">
  <input bind:this={input} value={displayValue} {id} {placeholder} {disabled} aria-label={label} role="combobox" aria-expanded={open} aria-controls="{id}-list" aria-autocomplete="list"
    spellcheck="false" autocomplete="off" onfocus={() => { typed = false; show(); }} onclick={show} oninput={event => { value = event.currentTarget.value; typed = true; active = -1; show(); }} onkeydown={key} onblur={() => (open = false)} />
  <button type="button" class="refselect-toggle" tabindex="-1" aria-label="Show references" {disabled}
    onpointerdown={event => event.preventDefault()} onclick={() => { if (open) open = false; else { typed = false; input.focus(); show(); } }}>
    <Icon name="disclosure" size={12} />
  </button>
  {#if open && box}
    <div class="refselect-list" id="{id}-list" role="listbox" tabindex="-1" onpointerdown={event => event.preventDefault()}
      style:left="{box.left}px" style:width="{box.width}px"
      style:top={upward ? undefined : `${box.bottom + 4}px`} style:bottom={upward ? `${innerHeight - box.top + 4}px` : undefined}
      style:max-height="{Math.max(120, Math.min(280, (upward ? box.top : room) - 16))}px">
      {#each rows as row, position (position)}
        {#if row.kind === 'head'}
          <div class="refselect-head">{row.label}</div>
        {:else}
          <div class="refselect-opt" class:on={row.index === active} role="option" tabindex="-1" aria-selected={row.index === active} data-index={row.index}
            onpointerenter={() => (active = row.index)} onclick={() => choose(row.name)} onkeydown={() => {}}>
            <Icon name={row.icon} size={12} tone={row.tone} /><span>{#if row.name === 'HEAD'}HEAD · current commit{:else}{#each segments(row.label, row.positions) as part}{#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}{/if}</span>{#if row.note}<small>{row.note}</small>{/if}
          </div>
        {/if}
      {:else}
        <div class="refselect-empty">No match. The text is used as typed, so a commit id works too.</div>
      {/each}
    </div>
  {/if}
</div>
