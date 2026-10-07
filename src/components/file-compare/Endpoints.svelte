<script lang="ts">
  import type { CompareEndpoint, EditFile } from '../../lib/api';
  import type { Command } from '../../lib/commands';
  import { formatLabel, type TextFormat } from '../../lib/text-format';
  import { refLabel } from '../../lib/compare-view';
  import Icon from '../Icon.svelte';

  let { endpoints, tickets, formats, dirty, reasons = [], saveCommands, onexecute }: {
    endpoints: CompareEndpoint[]; tickets: (EditFile | null)[]; formats: TextFormat[]; dirty: boolean[];
    reasons?: (string | null)[]; saveCommands: Command[]; onexecute: (command: Command) => void;
  } = $props();
</script>

<div class="editor-endpoints">{#each endpoints as endpoint, index}
    <div><strong>{index === 0 ? 'Left' : 'Right'} @ {refLabel(endpoint.reference)}</strong><span class="grow"></span>
      <span class="faint" title={reasons[index] ?? undefined}>{tickets[index] ? formatLabel(formats[index]) : 'Read-only'}{dirty[index] ? ' · Unsaved' : ''}</span>
      <button class="btn" title={saveCommands[index].reason ?? 'Save file'} disabled={!saveCommands[index].enabled} onclick={() => onexecute(saveCommands[index])}><Icon name="check" /> Save</button>
    </div>{/each}</div>
