<script lang="ts">
  import { app } from '../lib/state.svelte';
  import ActivityPanel from './panel/ActivityPanel.svelte';
  import ComparePanel from './panel/ComparePanel.svelte';
  import RecoveryEntry from './panel/RecoveryEntry.svelte';
  import Sidebar from './Sidebar.svelte';

  const MIN = 190, MAX = 360;
  const clamp = (width: number) => Math.round(Math.max(MIN, Math.min(MAX, width)));

  function resize(event: PointerEvent) {
    const element = event.currentTarget as HTMLElement;
    element.setPointerCapture(event.pointerId);
    const move = (next: PointerEvent) => { app.ws.shell.sidebarWidth = clamp(next.clientX - element.parentElement!.getBoundingClientRect().left); };
    const finish = () => {
      element.removeEventListener('pointermove', move);
      element.removeEventListener('pointerup', finish);
      element.removeEventListener('pointercancel', finish);
    };
    element.addEventListener('pointermove', move);
    element.addEventListener('pointerup', finish);
    element.addEventListener('pointercancel', finish);
  }
</script>

<aside class="side" aria-label="{app.ws.shell.section} panel">
  <button class="side-resize" aria-label="Resize panel" title="Drag to resize; double-click to reset" onpointerdown={resize}
    ondblclick={() => (app.ws.shell.sidebarWidth = 250)} onkeydown={event => {
      if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
        event.preventDefault();
        app.ws.shell.sidebarWidth = clamp(app.ws.shell.sidebarWidth + (event.key === 'ArrowRight' ? 20 : -20));
      }
    }}></button>
  {#if app.ws.shell.section === 'compare'}<ComparePanel />
  {:else if app.ws.shell.section === 'recovery'}<RecoveryEntry />
  {:else if app.ws.shell.section === 'activity'}<ActivityPanel />
  {:else}<Sidebar />{/if}
</aside>
