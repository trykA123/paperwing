<script lang="ts">
  import type { ChangeCounts } from '../../lib/change-summary';
  import { LANGUAGES, PLAIN_LABEL, PLAIN_TEXT } from '../../lib/languages';
  import Select, { type SelectOption } from '../Select.svelte';

  type Language = { id: string; label: string; enabled: boolean };
  let { counts, inline = $bindable(), hideSame = $bindable(), ignoreWhitespace = $bindable(), language, onlanguage }: {
    counts: ChangeCounts; inline: boolean; hideSame: boolean; ignoreWhitespace: boolean; language: Language; onlanguage: (id: string) => void;
  } = $props();
  const options = $derived<SelectOption[]>([{ id: PLAIN_TEXT, label: PLAIN_LABEL }, ...LANGUAGES].map(option => ({ value: option.id, label: option.id === language.id ? language.label : option.label })));
</script>

<footer class="fc-foot">
  <span class="fc-legend fc-add"><b aria-hidden="true">+</b>{counts.add} added</span>
  <span class="fc-legend fc-rem"><b aria-hidden="true">&minus;</b>{counts.rem} removed</span>
  <span class="fc-legend fc-chg"><b aria-hidden="true">~</b>{counts.chg} changed</span>
  <span class="grow"></span>
  <label class="check"><input type="checkbox" bind:checked={hideSame} /> Hide unchanged</label>
  <label class="check"><input type="checkbox" bind:checked={ignoreWhitespace} /> Ignore whitespace</label>
  <div class="seg small"><button class:on={!inline} onclick={() => inline = false}>Side by side</button><button class:on={inline} onclick={() => inline = true}>Inline</button></div>
  <Select class="language-pick" label="Language" searchable searchPlaceholder="Find a language" {options} value={language.id} disabled={!language.enabled} onchange={onlanguage} />
</footer>
