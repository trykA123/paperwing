<script lang="ts">
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';
  import { app } from '../lib/state.svelte';
  import type { NoticeItem, NoticeKind } from '../lib/notifications.svelte';
  import { motionMs } from '../lib/appearance';
  import BrandMark from './BrandMark.svelte';
  import Icon, { type IconName } from './Icon.svelte';

  const ICON: Record<Exclude<NoticeKind, 'loading'>, IconName> = { info: 'info', success: 'check', warn: 'alert', error: 'error' };
  const LABEL: Record<NoticeKind, string> = { info: 'Info', success: 'Success', warn: 'Warning', error: 'Error', loading: 'In progress' };

  let host: HTMLElement;
  const calm = $derived(app.notices.items.filter(item => item.kind !== 'error'));
  const errors = $derived(app.notices.items.filter(item => item.kind === 'error'));

  const raise = () => { if (host.matches(':popover-open')) host.hidePopover(); host.showPopover(); };

  function place() {
    const dialogs = document.querySelectorAll('dialog:modal');
    const parent = dialogs.length ? dialogs[dialogs.length - 1] : document.body;
    if (host.parentElement === parent && host.matches(':popover-open')) return;
    parent.append(host);
    raise();
  }

  onMount(() => {
    place();
    const watch = new MutationObserver(place);
    watch.observe(document.body, { attributes: true, attributeFilter: ['open'], childList: true, subtree: true });
    return () => watch.disconnect();
  });
</script>

{#snippet notice(notice: NoticeItem)}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="notice {notice.kind}" class:has-detail={!!notice.detail}
    in:fly={{ y: 12, duration: motionMs(180) }} out:fly={{ y: 8, duration: motionMs(140) }}
    onpointerenter={() => app.notices.hold(notice.id, 'pointer')} onpointerleave={() => app.notices.release(notice.id, 'pointer')}
    onfocusin={() => app.notices.hold(notice.id, 'focus')} onfocusout={() => app.notices.release(notice.id, 'focus')}
    onkeydown={event => { if (event.key === 'Escape') { event.stopPropagation(); app.notices.dismiss(notice.id); } }}>
    <span class="notice-icon">
      {#if notice.kind === 'loading'}<BrandMark busy size={20} />{:else}<Icon name={ICON[notice.kind]} size={16} />{/if}
    </span>
    <div class="notice-body">
      <span class="notice-msg"><span class="sr-only">{LABEL[notice.kind]}: </span>{notice.msg}</span>
      {#if notice.detail}<span class="notice-detail">{notice.detail}</span>{/if}
      {#if notice.actions.length}
        <div class="notice-actions">
          {#each notice.actions as action, index}
            <button class="notice-action" class:ghost={index > 0} onclick={() => app.notices.act(notice.id, action)}>{action.label}</button>
          {/each}
        </div>
      {/if}
    </div>
    <button class="notice-close" aria-label="Dismiss notification" onclick={() => app.notices.dismiss(notice.id)}><Icon name="close" size={12} /></button>
  </div>
{/snippet}

<div class="notices" popover="manual" role="region" aria-label="Notifications" bind:this={host}>
  <div class="notice-stack" aria-live="polite">
    {#each calm as item (item.id)}{@render notice(item)}{/each}
  </div>
  <div class="notice-stack" role="alert" aria-live="assertive">
    {#if errors.length > 1}
      <button class="notice-clear" onclick={() => app.notices.dismissErrors()}>Dismiss all errors</button>
    {/if}
    {#each errors as item (item.id)}{@render notice(item)}{/each}
  </div>
</div>


