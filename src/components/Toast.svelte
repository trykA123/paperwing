<script lang="ts">
  import { flip } from 'svelte/animate';
  import { fly } from 'svelte/transition';
  import { app, type ToastKind } from '../lib/state.svelte';
  import { motionMs } from '../lib/appearance';
  import Icon, { type IconName } from './Icon.svelte';

  const ICON: Record<ToastKind, IconName> = { info: 'info', success: 'check', warn: 'alert', error: 'error' };
</script>

<div class="toasts" aria-live="polite">
  {#each app.toasts as toast (toast.id)}
    <div class="toast {toast.kind}" role={toast.kind === 'error' ? 'alert' : undefined}
      animate:flip={{ duration: motionMs(160) }}
      in:fly={{ x: 24, duration: motionMs(180) }} out:fly={{ x: 24, duration: motionMs(140) }}
      onpointerenter={() => app.holdToast(toast.id)} onpointerleave={() => app.releaseToast(toast.id)}>
      <span class="toast-icon"><Icon name={ICON[toast.kind]} size={16} /></span>
      <span class="toast-msg">{toast.msg}</span>
      {#if toast.action}
        <button class="toast-action" onclick={() => { toast.action?.run(); app.dismissToast(toast.id); }}>{toast.action.label}</button>
      {/if}
      <button class="toast-close" aria-label="Dismiss notification" onclick={() => app.dismissToast(toast.id)}><Icon name="close" size={12} /></button>
    </div>
  {/each}
</div>
