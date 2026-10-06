<script lang="ts">
  import type { CompareEndpoint, EditFile } from '../../lib/api';
  import type { Command } from '../../lib/commands';
  import type { TextFormat } from '../../lib/editor';
  import { refLabel } from '../../lib/compare-view';
  import Icon from '../Icon.svelte';

  let { endpoints, tickets, formats, dirty, reasons = [], saveCommands, onexecute }: {
    endpoints: CompareEndpoint[]; tickets: (EditFile | null)[]; formats: TextFormat[]; dirty: boolean[];
    reasons?: (string | null)[]; saveCommands: Command[]; onexecute: (command: Command) => void;
  } = $props();
</script>

<div class="editor-endpoints">{#each endpoints as endpoint, index}
    <div><strong>{index === 0 ? 'Left' : 'Right'} @ {refLabel(endpoint.reference)}</strong><span class="grow"></span>
      <span class="faint" title={reasons[index] ?? undefined}>{tickets[index] ? formats[index]?.eol === '\r\n' ? 'UTF-8 · CRLF' : formats[index]?.eol === '\r' ? 'UTF-8 · CR' : 'UTF-8 · LF' : 'Read-only'}{dirty[index] ? ' · Unsaved' : ''}</span>
      <button class="btn" title={saveCommands[index].reason ?? 'Save file'} disabled={!saveCommands[index].enabled} onclick={() => onexecute(saveCommands[index])}><Icon name="check" /> Save</button>
    </div>{/each}</div>
