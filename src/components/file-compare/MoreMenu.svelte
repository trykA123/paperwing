<script lang="ts">
  import type { Command } from '../../lib/commands';
  import Icon from '../Icon.svelte';
  import Popover from '../Popover.svelte';

  type Remember = { extension: string; on: boolean };
  let { fileLeft, fileRight, undo, remember, onremember, onexecute }: {
    fileLeft: Command; fileRight: Command; undo: Command; remember: Remember | null; onremember: (on: boolean) => void; onexecute: (command: Command) => void;
  } = $props();
  let anchor = $state<HTMLButtonElement>();
  let open = $state(false);
  const run = (command: Command) => { open = false; onexecute(command); };
</script>

<button class="btn icon-only" bind:this={anchor} title="More file actions" aria-label="More file actions" aria-haspopup="dialog" aria-expanded={open} onclick={() => (open = !open)}><Icon name="more" size={16} /></button>
{#if open && anchor}
  <Popover {anchor} label="File actions" onclose={() => (open = false)} width={260}>
    <div class="menu-list" role="group" aria-label="File actions">
      <button class="menu-item" title={fileLeft.reason ?? undefined} disabled={!fileLeft.enabled} onclick={() => run(fileLeft)}><Icon name="copy" /><span class="lbl">Copy whole file to left</span></button>
      <button class="menu-item" title={fileRight.reason ?? undefined} disabled={!fileRight.enabled} onclick={() => run(fileRight)}><Icon name="copy" /><span class="lbl">Copy whole file to right</span></button>
      <button class="menu-item" title={undo.reason ?? undefined} disabled={!undo.enabled} onclick={() => run(undo)}><Icon name="refresh" /><span class="lbl">Undo saved operation</span></button>
      {#if remember}<hr /><label class="menu-item"><input type="checkbox" checked={remember.on} onchange={event => onremember(event.currentTarget.checked)} /><span class="lbl">Use this language for all .{remember.extension} files</span></label>{/if}
    </div>
  </Popover>
{/if}
