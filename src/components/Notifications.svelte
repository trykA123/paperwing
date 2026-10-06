<script lang="ts">
  import { flip } from 'svelte/animate';
  import { fly } from 'svelte/transition';
  import { app } from '../lib/state.svelte';
  import type { NoticeKind } from '../lib/notifications.svelte';
  import { motionMs } from '../lib/appearance';
  import BrandMark from './BrandMark.svelte';
  import Icon, { type IconName } from './Icon.svelte';

  const ICON: Record<Exclude<NoticeKind, 'loading'>, IconName> = { info: 'info', success: 'check', warn: 'alert', error: 'error' };
  const LABEL: Record<NoticeKind, string> = { info: 'Info', success: 'Success', warn: 'Warning', error: 'Error', loading: 'In progress' };
</script>

<div class="notices" role="region" aria-label="Notifications">
  {#each app.notices.items as notice (notice.id)}
    <div class="notice {notice.kind}" role={notice.kind === 'error' ? 'alert' : 'status'} class:has-detail={!!notice.detail}
      animate:flip={{ duration: motionMs(160) }}
      in:fly={{ y: 12, duration: motionMs(180) }} out:fly={{ y: 8, duration: motionMs(140) }}
      onpointerenter={() => app.notices.hold(notice.id)} onpointerleave={() => app.notices.release(notice.id)}
      onfocusin={() => app.notices.hold(notice.id)} onfocusout={() => app.notices.release(notice.id)}
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
  {/each}
</div>
