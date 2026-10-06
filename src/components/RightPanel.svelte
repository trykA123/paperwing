<script lang="ts">
  import { app } from '../lib/state.svelte';
  import { needsClone } from '../lib/row-actions';
  import CloneFooter from './right/CloneFooter.svelte';
  import CloneSettings from './right/CloneSettings.svelte';
  import ItemDetails from './right/ItemDetails.svelte';

  const detail = $derived(app.detailItem);
  const toClone = $derived(app.actionItems.filter(needsClone));

  const DEFAULT_W = 380;
  const clampW = (w: number) => Math.round(Math.max(280, Math.min(innerWidth - 48 - (app.ws.shell.sidebarVisible ? app.ws.shell.sidebarWidth : 0) - 420, w)));

  function startResize(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => (app.ws.rightWidth = clampW(innerWidth - ev.clientX));
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  }

  function resizeKey(e: KeyboardEvent) {
    if (e.key === 'ArrowLeft') app.ws.rightWidth = clampW(app.ws.rightWidth + 20);
    else if (e.key === 'ArrowRight') app.ws.rightWidth = clampW(app.ws.rightWidth - 20);
  }
</script>

<section class="right">
  <button class="rresize" aria-label="Resize panel (drag, or use arrow keys; double-click to reset)" title="Drag to resize · double-click to reset"
    onpointerdown={startResize} ondblclick={() => (app.ws.rightWidth = DEFAULT_W)} onkeydown={resizeKey}></button>
  <div class="rscroll">
    {#if detail}{#key detail.id}<ItemDetails item={detail} />{/key}{/if}
    {#if toClone.length}
      <CloneSettings items={toClone} />
    {:else if !detail}
      <div class="rsec"><p class="mut">Select a repository to see its details and history. Clone settings appear here when something is waiting to be cloned.</p></div>
    {/if}
  </div>
  {#if toClone.length}<CloneFooter items={toClone} />{/if}
</section>
