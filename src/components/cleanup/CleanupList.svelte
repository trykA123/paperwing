<script lang="ts">
  import { formatCommitDate, type CleanupRow } from '../../lib/branch-cleanup';
  import Icon from '../Icon.svelte';

  let { label, rows, picked, disabled, ontoggle, onall }: {
    label: string; rows: CleanupRow[]; picked: string[]; disabled: boolean;
    ontoggle: (name: string, on: boolean) => void; onall: (on: boolean) => void;
  } = $props();

  const open = $derived(rows.filter(row => !row.blocked));
  const blocked = $derived(rows.filter(row => row.blocked));
</script>

{#snippet line(row: CleanupRow)}
  <label class="cleanup-row" class:blocked={!!row.blocked}>
    <input type="checkbox" checked={picked.includes(row.name)} disabled={disabled || !!row.blocked} onchange={event => ontoggle(row.name, event.currentTarget.checked)} />
    <span class="cleanup-name mono" title={row.name}>{row.name}</span>
    {#if row.blocked}<span class="cleanup-tag">{row.blocked}</span>{/if}
    {#each row.notes as note (note)}<span class="cleanup-tag warn"><Icon name="alert" size={11} />{note}</span>{/each}
    <span class="cleanup-subject mut" title={row.subject}>{row.subject}</span>
    <span class="cleanup-date mut">{formatCommitDate(row.lastCommit)}</span>
  </label>
{/snippet}

<div class="cleanup-list" role="group" aria-label={label}>
  <div class="cleanup-list-head">
    <b>{label}</b><span class="mut">{picked.length} of {open.length} selected</span><span class="grow"></span>
    <button type="button" class="btn small" disabled={disabled || !open.length} onclick={() => onall(true)}>Select all</button>
    <button type="button" class="btn small" disabled={disabled || !picked.length} onclick={() => onall(false)}>Clear</button>
  </div>
  {#each open as row (row.name)}{@render line(row)}{:else}<p class="cleanup-empty mut">Nothing to clean up here.</p>{/each}
  {#if blocked.length}
    <details class="cleanup-blocked"><summary>Kept ({blocked.length})</summary>
      {#each blocked as row (row.name)}{@render line(row)}{/each}
    </details>
  {/if}
</div>
