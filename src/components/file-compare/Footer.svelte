<script lang="ts">
  import type { Command } from '../../lib/commands';
  import Icon from '../Icon.svelte';

  let { actions, onexecute }: {
    actions: { hunkLeft: Command; hunkRight: Command; fileLeft: Command; fileRight: Command; undo: Command };
    onexecute: (command: Command) => void;
  } = $props();
</script>

<footer class="editor-footer"><button class="btn" title={actions.hunkLeft.reason ?? 'Copy selected hunk to left (Ctrl+Alt+Left)'} disabled={!actions.hunkLeft.enabled} onclick={() => onexecute(actions.hunkLeft)}><Icon name="copy" /> To left</button>
    <button class="btn" title={actions.hunkRight.reason ?? 'Copy selected hunk to right (Ctrl+Alt+Right)'} disabled={!actions.hunkRight.enabled} onclick={() => onexecute(actions.hunkRight)}><Icon name="copy" /> To right</button>
    <button class="btn" title={actions.fileLeft.reason ?? 'Copy file to left'} disabled={!actions.fileLeft.enabled} onclick={() => onexecute(actions.fileLeft)}><Icon name="copy" /> File to left</button>
    <button class="btn" title={actions.fileRight.reason ?? 'Copy file to right'} disabled={!actions.fileRight.enabled} onclick={() => onexecute(actions.fileRight)}><Icon name="copy" /> File to right</button>
    <span class="grow"></span><button class="btn" title={actions.undo.reason ?? 'Undo saved operation'} disabled={!actions.undo.enabled} onclick={() => onexecute(actions.undo)}><Icon name="refresh" /> Undo saved operation</button></footer>
