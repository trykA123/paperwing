<script lang="ts">
  import { onMount } from 'svelte';

  const EDGE = 8;
  const GAP = 6;
  const SIDE_GAP = 8;
  const CHAINED_MS = 300;

  let tip = $state<{ text: string; rect: DOMRect; side: boolean } | null>(null);
  let host: HTMLDivElement;
  let target: HTMLElement | null = null;
  let pressed: HTMLElement | null = null;
  let timer = 0;
  let closedAt = 0;

  // The native title is moved to data-tip while shown so the OS tooltip does not double up.
  function restore() {
    clearTimeout(timer);
    if (target?.dataset.tip !== undefined) { target.title = target.dataset.tip; delete target.dataset.tip; }
    target = null;
    queueMicrotask(() => {
      if (tip) closedAt = performance.now();
      tip = null;
    });
  }

  function press() {
    pressed = target ?? pressed;
    restore();
  }

  function place() {
    if (!tip) return;
    const { rect, side } = tip;
    host.style.left = '0px';
    host.style.top = '0px';
    const width = host.offsetWidth;
    const height = host.offsetHeight;
    const clampX = (x: number) => Math.max(EDGE, Math.min(innerWidth - width - EDGE, x));
    const clampY = (y: number) => Math.max(EDGE, Math.min(innerHeight - height - EDGE, y));
    if (side && rect.right + SIDE_GAP + width + EDGE <= innerWidth) {
      host.style.left = `${rect.right + SIDE_GAP}px`;
      host.style.top = `${clampY(rect.top + rect.height / 2 - height / 2)}px`;
      return;
    }
    const above = rect.bottom + GAP + height + EDGE > innerHeight;
    host.style.left = `${clampX(rect.left + rect.width / 2 - width / 2)}px`;
    host.style.top = `${clampY(above ? rect.top - GAP - height : rect.bottom + GAP)}px`;
  }

  function show() {
    if (!target?.isConnected) return restore();
    tip = { text: target.dataset.tip ?? '', rect: target.getBoundingClientRect(), side: target.dataset.tipSide === 'right' };
  }

  function enter(event: Event) {
    const el = (event.target as Element | null)?.closest?.<HTMLElement>('[title]');
    if (!el?.title || el === target || el === pressed) return;
    restore();
    target = el;
    el.dataset.tip = el.title;
    el.removeAttribute('title');
    const chained = performance.now() - closedAt < CHAINED_MS;
    timer = window.setTimeout(show, event.type === 'focusin' || chained ? 0 : 450);
  }

  function leave(event: Event) {
    const next = (event as PointerEvent).relatedTarget as Node | null;
    if (pressed && !pressed.contains(next)) pressed = null;
    if (target && !target.contains(next)) restore();
  }

  $effect(() => {
    if (!host) return;
    const open = host.matches(':popover-open');
    if (tip && !open) host.showPopover();
    else if (!tip && open) host.hidePopover();
    if (tip) place();
  });

  onMount(() => {
    const options = { capture: true } as const;
    document.addEventListener('pointerover', enter, options);
    document.addEventListener('focusin', enter, options);
    document.addEventListener('pointerout', leave, options);
    document.addEventListener('focusout', restore, options);
    document.addEventListener('pointerdown', press, options);
    document.addEventListener('keydown', restore, options);
    document.addEventListener('scroll', restore, options);
    return () => {
      restore();
      document.removeEventListener('pointerover', enter, options);
      document.removeEventListener('focusin', enter, options);
      document.removeEventListener('pointerout', leave, options);
      document.removeEventListener('focusout', restore, options);
      document.removeEventListener('pointerdown', press, options);
      document.removeEventListener('keydown', restore, options);
      document.removeEventListener('scroll', restore, options);
    };
  });
</script>

<div bind:this={host} class="tooltip" popover="manual" role="tooltip">{tip?.text}</div>
