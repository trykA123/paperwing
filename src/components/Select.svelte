<script lang="ts" module>
  export type SelectOption = { value: string; label: string; group?: string; hint?: string };
</script>

<script lang="ts">
  import { tick } from 'svelte';
  import { rank, segments } from '../lib/fuzzy';
  import Icon from './Icon.svelte';

  let { value, options, label, disabled = false, searchable = false, placeholder = 'Select\u2026', searchPlaceholder = 'Search\u2026', class: klass = '', onchange }: {
    value: string; options: SelectOption[]; label: string; disabled?: boolean; searchable?: boolean; placeholder?: string;
    searchPlaceholder?: string; class?: string; onchange: (value: string) => void;
  } = $props();

  const uid = $props.id();
  let wrap: HTMLDivElement;
  let trigger: HTMLButtonElement;
  let search = $state<HTMLInputElement>();
  let open = $state(false);
  let query = $state('');
  let active = $state(0);
  let box = $state<{ left: number; width: number; top: number; bottom: number } | null>(null);

  const current = $derived(options.find(option => option.value === value));
  type Row = { kind: 'head'; label: string } | { kind: 'opt'; option: SelectOption; positions: number[]; index: number };
  const rows = $derived.by(() => {
    const out: Row[] = [];
    let group: string | undefined;
    let index = 0;
    for (const { item, positions } of rank(query, options, option => option.label)) {
      if (!query.trim() && item.group !== group) { group = item.group; if (group) out.push({ kind: 'head', label: group }); }
      out.push({ kind: 'opt', option: item, positions, index: index++ });
    }
    return out;
  });
  const choices = $derived(rows.filter((row): row is Extract<Row, { kind: 'opt' }> => row.kind === 'opt'));
  const room = $derived(box ? innerHeight - box.bottom : 0);
  const upward = $derived(!!box && room < 240 && box.top > room);

  async function show() {
    if (disabled) return;
    const rect = trigger.getBoundingClientRect();
    box = { left: rect.left, width: rect.width, top: rect.top, bottom: rect.bottom };
    query = '';
    active = Math.max(0, options.findIndex(option => option.value === value));
    open = true;
    await tick();
    search?.focus();
    reveal();
  }

  function close(refocus = false) {
    open = false;
    if (refocus) trigger.focus();
  }

  function choose(next: string) {
    close(true);
    if (next !== value) onchange(next);
  }

  function reveal() {
    document.querySelector(`#${uid}-list [data-index="${active}"]`)?.scrollIntoView({ block: 'nearest' });
  }

  function move(to: number) {
    if (!choices.length) return;
    active = (to + choices.length) % choices.length;
    queueMicrotask(reveal);
  }

  function key(event: KeyboardEvent) {
    if (!open) {
      if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(event.key) && event.target === trigger) { event.preventDefault(); void show(); }
      return;
    }
    if (event.key === 'ArrowDown') { event.preventDefault(); move(active + 1); }
    else if (event.key === 'ArrowUp') { event.preventDefault(); move(active - 1); }
    else if (event.key === 'Home') { event.preventDefault(); move(0); }
    else if (event.key === 'End') { event.preventDefault(); move(choices.length - 1); }
    else if (event.key === 'Enter' || (event.key === ' ' && event.target === trigger)) { event.preventDefault(); if (choices[active]) choose(choices[active].option.value); }
    else if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(true); }
    else if (event.key === 'Tab') close();
  }
</script>

<div class="sel {klass}" role="presentation" bind:this={wrap} onkeydown={key} onfocusout={event => { if (!wrap.contains(event.relatedTarget as Node | null)) open = false; }}>
  <button bind:this={trigger} type="button" class="sel-trigger" class:open role="combobox" aria-haspopup="listbox" aria-expanded={open} aria-controls="{uid}-list"
    aria-label={label} {disabled} title={current?.label} onclick={() => (open ? close() : void show())}>
    <span class:placeholder={!current}>{current?.label ?? placeholder}</span>
    <Icon name="disclosure" size={12} />
  </button>
  {#if open && box}
    <div class="refselect-list sel-list" id="{uid}-list" role="listbox" tabindex="-1"
      onpointerdown={event => { if (!(event.target as Element).closest('input')) event.preventDefault(); }}
      style:left="{box.left}px" style:min-width="{box.width}px" style:max-width="min(480px, calc(100vw - {box.left}px - 16px))"
      style:top={upward ? undefined : `${box.bottom + 4}px`} style:bottom={upward ? `${innerHeight - box.top + 4}px` : undefined}
      style:max-height="{Math.max(160, Math.min(360, (upward ? box.top : room) - 16))}px">
      {#if searchable}
        <div class="sel-search">
          <Icon name="search" size={13} />
          <input bind:this={search} bind:value={query} placeholder={searchPlaceholder} aria-label="{label} search" spellcheck="false" autocomplete="off"
            oninput={() => (active = 0)} />
        </div>
      {/if}
      <div class="sel-rows">
        {#each rows as row, position (position)}
          {#if row.kind === 'head'}
            <div class="refselect-head">{row.label}</div>
          {:else}
            <div class="refselect-opt" class:on={row.index === active} class:current={row.option.value === value} role="option" tabindex="-1"
              aria-selected={row.option.value === value} data-index={row.index} onpointerenter={() => (active = row.index)}
              onclick={() => choose(row.option.value)} onkeydown={() => {}}>
              <span>{#each segments(row.option.label, row.positions) as part}{#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}</span>
              {#if row.option.hint || (query.trim() && row.option.group)}<small>{row.option.hint ?? row.option.group}</small>{/if}
              {#if row.option.value === value}<Icon name="check" size={13} tone="brand" />{/if}
            </div>
          {/if}
        {:else}
          <div class="refselect-empty">No matches</div>
        {/each}
      </div>
    </div>
  {/if}
</div>
