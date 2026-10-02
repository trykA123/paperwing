<script lang="ts">
  import { onMount } from 'svelte';

  let tip = $state<{ text: string; x: number; y: number; above: boolean } | null>(null);
  let host: HTMLDivElement;
  let target: HTMLElement | null = null;
  let timer = 0;

  // The native title is moved to data-tip while shown so the OS tooltip does not double up.
  function restore() {
    clearTimeout(timer);
    if (target?.dataset.tip !== undefined) { target.title = target.dataset.tip; delete target.dataset.tip; }
    target = null;
    tip = null;
  }

  function show() {
    if (!target?.isConnected) return restore();
    const rect = target.getBoundingClientRect();
    const above = rect.bottom + 48 > innerHeight;
    tip = { text: target.dataset.tip ?? '', x: Math.max(150, Math.min(innerWidth - 150, rect.left + rect.width / 2)), y: above ? rect.top - 6 : rect.bottom + 6, above };
  }

  function enter(event: Event) {
    const el = (event.target as Element | null)?.closest?.<HTMLElement>('[title]');
    if (!el?.title || el === target) return;
    restore();
    target = el;
    el.dataset.tip = el.title;
    el.removeAttribute('title');
    timer = window.setTimeout(show, event.type === 'focusin' ? 0 : 450);
  }

  function leave(event: Event) {
    if (target && !target.contains((event as PointerEvent).relatedTarget as Node | null)) restore();
  }

  $effect(() => {
    if (!host) return;
    const open = host.matches(':popover-open');
    if (tip && !open) host.showPopover();
    else if (!tip && open) host.hidePopover();
  });

  onMount(() => {
    const options = { capture: true } as const;
    document.addEventListener('pointerover', enter, options);
    document.addEventListener('focusin', enter, options);
    document.addEventListener('pointerout', leave, options);
    document.addEventListener('focusout', restore, options);
    document.addEventListener('pointerdown', restore, options);
    document.addEventListener('keydown', restore, options);
    document.addEventListener('scroll', restore, options);
    return () => {
      restore();
      document.removeEventListener('pointerover', enter, options);
      document.removeEventListener('focusin', enter, options);
      document.removeEventListener('pointerout', leave, options);
      document.removeEventListener('focusout', restore, options);
      document.removeEventListener('pointerdown', restore, options);
      document.removeEventListener('keydown', restore, options);
      document.removeEventListener('scroll', restore, options);
    };
  });
</script>

<div bind:this={host} class="tooltip" class:above={tip?.above} popover="manual" role="tooltip"
  style:left="{tip?.x ?? 0}px" style:top="{tip?.y ?? 0}px">{tip?.text}</div>
