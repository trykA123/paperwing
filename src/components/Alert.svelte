<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon, { type IconName } from './Icon.svelte';

  type AlertKind = 'info' | 'warn' | 'err' | 'ok';
  let { kind = 'info', title, role, children, action }: { kind?: AlertKind; title?: string; role?: 'alert' | 'status'; children: Snippet; action?: Snippet } = $props();

  const ICON: Record<AlertKind, IconName> = { info: 'info', warn: 'alert', err: 'error', ok: 'check' };
  const LABEL: Record<AlertKind, string> = { info: 'Info', warn: 'Warning', err: 'Error', ok: 'Success' };
</script>

<div class="banner {kind}" {role}>
  <span class="banner-icon"><Icon name={ICON[kind]} size={16} /></span>
  <div class="banner-body">
    {#if title}<b class="banner-title">{title}</b>{/if}
    <span class="banner-text"><span class="sr-only">{LABEL[kind]}: </span>{@render children()}</span>
  </div>
  {#if action}<div class="banner-action">{@render action()}</div>{/if}
</div>
