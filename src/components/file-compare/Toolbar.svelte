<script lang="ts">
  import type { Command } from '../../lib/commands';
  import { LANGUAGES, PLAIN_LABEL, PLAIN_TEXT } from '../../lib/languages';
  import Icon from '../Icon.svelte';
  import Select, { type SelectOption } from '../Select.svelte';

  type LanguageChoice = { id: string; label: string; extension: string; remembered: boolean; enabled: boolean };
  let { path, count, previous, next, inline = $bindable(), hideSame = $bindable(), ignoreWhitespace = $bindable(), language, onlanguage, onremember, onexecute }: {
    path: string; count: string; previous: Command; next: Command;
    inline: boolean; hideSame: boolean; ignoreWhitespace: boolean; language: LanguageChoice;
    onlanguage: (id: string) => void; onremember: (on: boolean) => void; onexecute: (command: Command) => void;
  } = $props();
  const options = $derived<SelectOption[]>([{ id: PLAIN_TEXT, label: PLAIN_LABEL }, ...LANGUAGES].map(option => ({ value: option.id, label: option.id === language.id ? language.label : option.label })));
</script>

<header class="compare-summary editor-toolbar"><strong class="mono">{path}</strong><span class="grow"></span>
    <button class="btn small icon-only flip" title="Previous difference (Shift+F7)" aria-label="Previous difference" disabled={!previous.enabled} onclick={() => onexecute(previous)}><Icon name="chevron" /></button>
    <span class="editor-count">{count}</span>
    <button class="btn small icon-only" title="Next difference (F7)" aria-label="Next difference" disabled={!next.enabled} onclick={() => onexecute(next)}><Icon name="chevron" /></button>
    <div class="seg small"><button class:on={!inline} onclick={() => inline = false}>Side by side</button><button class:on={inline} onclick={() => inline = true}>Inline</button></div>
    <label class="check"><input type="checkbox" bind:checked={hideSame} /> Hide unchanged</label>
    <label class="check"><input type="checkbox" bind:checked={ignoreWhitespace} /> Ignore whitespace</label>
    <Select class="language-pick" label="Language" searchable searchPlaceholder="Find a language" {options} value={language.id} disabled={!language.enabled} onchange={onlanguage} />
    {#if language.extension && language.enabled}<label class="check"><input type="checkbox" checked={language.remembered} onchange={event => onremember(event.currentTarget.checked)} /> Use for all .{language.extension} files</label>{/if}
  </header>
