<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { containFocus, trapTab } from '../lib/focus-trap';

  let { anchor, label, onclose, width = 280, tall = false, children }: { anchor: Element; label: string; onclose: () => void; width?: number; tall?: boolean; children: Snippet } = $props();

  const GAP = 6;
  let panel: HTMLDivElement;
  let spot = $state({ left: 0, top: 0 });
  const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;

  function place() {
    const box = anchor.getBoundingClientRect();
    const left = Math.max(GAP, Math.min(box.left, innerWidth - width - GAP));
    const room = innerHeight - box.bottom - GAP;
    spot = { left, top: room > 240 || box.top < 240 ? box.bottom + GAP : Math.max(GAP, box.top - GAP - panel.offsetHeight) };
  }

  function close() { onclose(); if (opener?.isConnected) opener.focus(); }

  onMount(() => {
    place();
    const release = containFocus(panel);
    panel.querySelector<HTMLElement>('input, button:not(:disabled)')?.focus();
    const resize = () => place();
    addEventListener('resize', resize);
    return () => { release(); removeEventListener('resize', resize); };
  });
</script>

<svelte:window onpointerdown={event => { if (!panel.contains(event.target as Node) && !anchor.contains(event.target as Node)) onclose(); }}
  onkeydown={event => { if (event.key === 'Escape' && !document.querySelector('dialog[open]')) { event.preventDefault(); event.stopPropagation(); close(); } }} />

<div class="popover" class:tall role="dialog" aria-label={label} tabindex="-1" bind:this={panel} onkeydown={event => trapTab(event, panel)} style:left="{spot.left}px" style:top="{spot.top}px" style:width="{width}px">{@render children()}</div>
