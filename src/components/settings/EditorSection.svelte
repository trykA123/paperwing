<script lang="ts">
  import { LANGUAGES, normalizeExtension, PLAIN_LABEL, PLAIN_TEXT } from '../../lib/languages';
  import Icon from '../Icon.svelte';
  import Select, { type SelectOption } from '../Select.svelte';

  let { map = $bindable({}) }: { map?: Record<string, string> } = $props();
  const options: SelectOption[] = [{ id: PLAIN_TEXT, label: PLAIN_LABEL }, ...LANGUAGES].map(option => ({ value: option.id, label: option.label }));
  const rows = $derived(Object.entries(map).sort(([a], [b]) => a.localeCompare(b)));
  let extension = $state(''), language = $state('xml');
  const normalized = $derived(normalizeExtension(extension));
  const invalid = $derived(extension.trim() !== '' && !normalized);

  function add() {
    if (!normalized) return;
    map = { ...map, [normalized]: language };
    extension = '';
  }
  function remove(key: string) {
    const { [key]: _removed, ...rest } = map;
    map = rest;
  }
</script>

<section class="settings-section">
    <div class="section-head"><div class="grow"><h2>Editor</h2><p class="mut">Choose the syntax colouring for a file extension. Your choice wins over the built-in guess.</p></div></div>
    <div class="card setting-list">
      {#each rows as [key, id] (key)}
        <div class="setting-row mapping-row">
          <span class="setting-label"><b class="mono">.{key}</b></span>
          <Select label="Language for .{key} files" searchable searchPlaceholder="Find a language" {options} value={id} onchange={value => (map = { ...map, [key]: value })} />
          <button class="btn small icon-only" title="Remove .{key}" aria-label="Remove .{key} mapping" onclick={() => remove(key)}><Icon name="trash" /></button>
        </div>
      {:else}
        <div class="setting-row"><span class="setting-label"><b>No custom mappings</b><small>Built-in guesses are used for every extension.</small></span></div>
      {/each}
      <form class="setting-row mapping-row" onsubmit={event => { event.preventDefault(); add(); }}>
        <input class="mono" placeholder=".ext" aria-label="File extension" aria-invalid={invalid} bind:value={extension} maxlength="17" spellcheck="false" autocomplete="off" />
        <Select label="Language for the new mapping" searchable searchPlaceholder="Find a language" {options} value={language} onchange={value => (language = value)} />
        <button class="btn small" type="submit" disabled={!normalized}>Add mapping</button>
      </form>
    </div>
    {#if invalid}<p class="hint" role="status">Use letters, digits, - or _ only.</p>{/if}
  </section>
