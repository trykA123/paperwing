<script lang="ts">
  import { untrack } from 'svelte';
  import type { CompareRef } from '../lib/api';
  import { missingTreePaths, refChoices } from '../lib/compare-refs';
  import { app } from '../lib/state.svelte';
  import Icon from './Icon.svelte';
  import RefSelect, { type RefGroup } from './RefSelect.svelte';
  import Select, { type SelectOption } from './Select.svelte';

  let { reference = $bindable(), paths, label, disabled = false, readOnly = false }: {
    reference: CompareRef; paths: string[]; label: string; disabled?: boolean; readOnly?: boolean;
  } = $props();
  const uid = $props.id();
  const value = $derived('name' in reference ? reference.name : 'sha' in reference ? reference.sha : '');
  const loading = $derived(paths.some(path => app.trees[path]?.loading));
  const errors = $derived(paths.flatMap(path => app.trees[path]?.error ? [app.trees[path]!.error!] : []));
  const kinds = $derived<SelectOption[]>([
    { value: 'head', label: 'HEAD (current checkout)' },
    ...(readOnly ? [] : [{ value: 'workingTree', label: 'Working tree' }]),
    { value: 'branch', label: 'Local branch' }, { value: 'remoteBranch', label: 'Remote branch' },
    { value: 'tag', label: 'Tag' }, { value: 'commit', label: 'Commit SHA' },
  ]);
  const GROUPS = {
    branch: { label: 'Local branches', icon: 'branch', tone: 'branch' },
    remoteBranch: { label: 'Remote branches', icon: 'branch', tone: 'repo' },
    tag: { label: 'Tags', icon: 'tag', tone: 'tag' },
    commit: { label: 'Ref tips', icon: 'commit', tone: 'commit' },
  } as const;
  const PLACEHOLDERS = { branch: 'Search local branches', remoteBranch: 'Search remote branches', tag: 'Search tags', commit: 'Commit SHA or ref tip' };
  const groups = $derived.by((): RefGroup[] => {
    if (reference.kind === 'head' || reference.kind === 'workingTree') return [];
    const choices = refChoices(reference.kind, app.trees, paths);
    return [{ ...GROUPS[reference.kind], names: choices.map(choice => choice.value), labels: Object.fromEntries(choices.map(choice => [choice.value, choice.label])),
      notes: Object.fromEntries(choices.filter(choice => choice.note).map(choice => [choice.value, choice.note])) }];
  });
  $effect(() => {
    const current = [...new Set(paths)];
    const consumer = new AbortController();
    untrack(() => { for (const path of current) if (path) void app.loadTree(path, false, consumer.signal); });
    return () => consumer.abort();
  });
  const missing = $derived(JSON.stringify(missingTreePaths(paths, app.trees)));
  $effect(() => {
    const lost: string[] = JSON.parse(missing);
    untrack(() => { for (const path of lost) void app.loadTree(path, false); });
  });
  const empty = $derived(!loading && !errors.length && reference.kind !== 'head' && reference.kind !== 'workingTree' && groups[0]?.names.length === 0 && paths.every(path => app.trees[path]?.data));
  const EMPTY = { branch: 'No local branches', remoteBranch: 'No remote branches yet. Fetch this repository.', tag: 'No tags', commit: '' };
  function changeKind(kind: CompareRef['kind']) {
    reference = kind === 'head' || kind === 'workingTree' ? { kind }
      : kind === 'commit' ? { kind, sha: '' } : { kind, name: '' };
  }
  function changeValue(value: string) {
    if (reference.kind === 'commit') reference = { kind: 'commit', sha: value };
    else if (reference.kind !== 'head' && reference.kind !== 'workingTree') reference = { kind: reference.kind, name: value };
  }
</script>

<Select class="ref-kind" label="{label} reference type" {disabled} value={reference.kind} options={kinds} onchange={kind => changeKind(kind as CompareRef['kind'])} />
{#if reference.kind !== 'head' && reference.kind !== 'workingTree'}
  <div class="ref-field">
    <RefSelect id="{uid}-ref" label="{label} reference" {disabled} {groups} bind:value={() => value, changeValue}
      placeholder={loading ? 'Loading references…' : PLACEHOLDERS[reference.kind]} />
  </div>
  <button class="icon" title="Refresh reference list" aria-label="{label} refresh reference list" disabled={disabled || loading || !paths.length} onclick={() => { for (const path of new Set(paths)) void app.loadTree(path, true); }}><Icon name="refresh" /></button>
{:else}
  <span class="ref-field ref-fixed">{reference.kind === 'head' ? 'Whatever is checked out now' : 'Files on disk, including uncommitted changes'}</span>
{/if}
{#if empty && EMPTY[reference.kind as keyof typeof EMPTY]}<span class="hint">{EMPTY[reference.kind as keyof typeof EMPTY]}</span>{/if}
{#if errors.length}<span class="warn" title={errors.join('\n')}>Refs unavailable</span>{/if}
