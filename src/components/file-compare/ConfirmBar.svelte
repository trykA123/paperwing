<script lang="ts">
  import { onMount } from 'svelte';

  let { message, confirmLabel, onconfirm, oncancel }: { message: string; confirmLabel: string; onconfirm: () => void; oncancel: () => void } = $props();
  const id = $props.id();
  let confirmButton: HTMLButtonElement;
  onMount(() => confirmButton.focus());
</script>

<div class="confirm-bar" role="alertdialog" tabindex="-1" aria-label="Confirm copy" aria-describedby={id}
  onkeydown={event => { if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); oncancel(); } }}>
  <span class="confirm-text" {id}>{message}</span>
  <button class="btn dark" bind:this={confirmButton} onclick={onconfirm}>{confirmLabel}</button>
  <button class="btn" onclick={oncancel}>Cancel</button>
</div>
