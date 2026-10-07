<script lang="ts">
  import type { SearchMode } from '../../lib/api';
  import type { SearchForm } from '../../lib/search-request';
  import Icon from '../Icon.svelte';
  import Select from '../Select.svelte';

  let { form = $bindable(), perl, active, canSearch, refsSet, onsearch, onstop }: {
    form: SearchForm; perl: boolean; active: boolean; canSearch: boolean; refsSet: boolean; onsearch: () => void; onstop: () => void;
  } = $props();

  const modes = $derived([
    { value: 'fixed', label: 'Plain text' },
    { value: 'basic', label: 'Regular expression' },
    { value: 'perl', label: perl ? 'Perl regular expression' : 'Perl regular expression (not in this Git)' },
  ].filter(option => perl || option.value !== 'perl' || form.mode === 'perl'));
  const contexts = [0, 1, 2, 3].map(value => ({ value: String(value), label: value ? `${value} context ${value === 1 ? 'line' : 'lines'}` : 'No context' }));
</script>

<form class="cs-bar" onsubmit={event => { event.preventDefault(); if (canSearch) onsearch(); }}>
  <div class="cs-line">
    <label class="gsearch cs-pattern"><Icon name="search" /><span class="sr-only">Search pattern</span>
      <input bind:value={form.pattern} placeholder="Find in code" spellcheck="false" autocomplete="off" />
    </label>
    {#if active}
      <button type="button" class="btn danger" onclick={onstop}><Icon name="close" /> Stop</button>
    {:else}
      <button type="submit" class="btn dark" disabled={!canSearch}><Icon name="search" /> Search</button>
    {/if}
  </div>
  <div class="cs-options">
    <Select value={form.mode} label="Match mode" options={modes} onchange={value => { form.mode = value as SearchMode; }} />
    <Select value={String(form.context)} label="Context lines" options={contexts} onchange={value => { form.context = Number(value); }} />
    <label class="check"><input type="checkbox" bind:checked={form.ignoreCase} /> Ignore case</label>
    <label class="check"><input type="checkbox" bind:checked={form.wholeWord} /> Whole word</label>
    <label class="check" title={refsSet ? 'Untracked files exist only in the working tree' : undefined}><input type="checkbox" bind:checked={form.untracked} disabled={refsSet} /> Untracked files</label>
    <label class="fld cs-paths"><span class="sr-only">Path filters</span>
      <input bind:value={form.pathspecs} placeholder="Paths, comma separated: src/*.ts, :!vendor" spellcheck="false" autocomplete="off" />
    </label>
  </div>
</form>
