<script lang="ts">
  import { onMount } from 'svelte';
  import { answer, type ConfirmRequest } from '../lib/confirm';
  import { dialogOut } from '../lib/motion';
  import Icon, { type IconName, type IconTone } from './Icon.svelte';

  let { request }: { request: ConfirmRequest } = $props();
  let dialog: HTMLDialogElement;
  let cancelButton: HTMLButtonElement;
  let okButton: HTMLButtonElement;
  let checked = $state(false);

  const look = $derived<{ icon: IconName; tone: IconTone; box: string }>(
    request.kind === 'error' || request.destructive ? { icon: request.kind === 'error' ? 'error' : 'alert', tone: 'err', box: 'err' }
    : request.kind === 'info' ? { icon: 'info', tone: 'brand', box: 'info' }
    : { icon: 'alert', tone: 'warn', box: 'warn' });

  onMount(() => {
    dialog.showModal();
    (request.destructive ? cancelButton : okButton).focus();
  });
</script>

<dialog class="operation-dialog confirm-dialog" bind:this={dialog} out:dialogOut|global aria-label={request.title ?? 'Confirm'}
  oncancel={event => { event.preventDefault(); answer(false); }}>
  <div class="confirm-body">
    <span class="confirm-icon {look.box}"><Icon name={look.icon} tone={look.tone} size={20} /></span>
    <div>
      <h2>{request.title ?? 'Confirm'}</h2>
      <p>{request.message}</p>
      {#if request.check}
        <label class="confirm-check"><input type="checkbox" bind:checked /><span><b>{request.check.label}</b>{#if request.check.hint}<small>{request.check.hint}</small>{/if}</span></label>
      {/if}
    </div>
  </div>
  <footer>
    <span class="grow"></span>
    <button class="btn" bind:this={cancelButton} onclick={() => answer(false)}>{request.cancelLabel ?? 'Cancel'}</button>
    <button class="btn {request.destructive ? 'danger' : 'dark'}" bind:this={okButton} onclick={() => answer(true, checked)}>{request.okLabel ?? 'OK'}</button>
  </footer>
</dialog>
