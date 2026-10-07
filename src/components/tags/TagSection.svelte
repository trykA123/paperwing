<script lang="ts">
  import { tick } from 'svelte';
  import { api, type TagInfo } from '../../lib/api';
  import { describeError } from '../../lib/errors';
  import { disabledReason } from '../../lib/menu-reason';
  import { plural } from '../../lib/plural';
  import { pathFacts } from '../../lib/stash-flow.svelte';
  import { tagFlow, tagTarget } from '../../lib/tag-flow.svelte';
  import { compareTagsDesc, shortId } from '../../lib/tags-set';
  import Alert from '../Alert.svelte';
  import Icon from '../Icon.svelte';

  let { path, name, tags = $bindable([]) }: { path: string; name: string; tags?: TagInfo[] } = $props();
  let loaded = $state(false);
  let loadError = $state('');
  let showAll = $state(false);
  let newButton = $state<HTMLButtonElement>();
  let sequence = 0;
  const CAP = 50;

  const reason = $derived(disabledReason(['cloned'], pathFacts(path)));
  const target = $derived(tagTarget(path, name));
  const sorted = $derived([...tags].sort((a, b) => compareTagsDesc(a.name, b.name)));
  const shown = $derived(showAll ? sorted : sorted.slice(0, CAP));

  async function load() {
    const mine = ++sequence;
    try {
      const next = await api.listTags(path);
      if (mine !== sequence) return;
      tags = next; loadError = '';
    } catch (error) {
      if (mine !== sequence) return;
      loadError = describeError(error, 'list the tags');
    }
    loaded = true;
    await tick();
    const active = document.activeElement;
    if ((!active || active === document.body || !active.isConnected) && !document.querySelector('dialog[open]')) newButton?.focus();
  }
  $effect(() => { void tagFlow.revision; void path; void load(); });
</script>

<section class="tag-section" aria-label="Tags">
  <header class="tag-head">
    <h3><Icon name="tag" size={14} tone="tag" />Tags {#if loaded && !loadError}<small>{tags.length}</small>{/if}</h3>
    <button class="btn small" data-tag-new bind:this={newButton} disabled={!!reason} title={reason ?? 'Tag the current commit'} onclick={event => tagFlow.openCreateFor([target], event.currentTarget)}>New tag…</button>
  </header>
  {#if loadError}
    <Alert kind="err" role="alert">{loadError}{#snippet action()}<button class="btn small" onclick={load}>Retry</button>{/snippet}</Alert>
  {:else if !loaded}
    <p class="mut tag-empty">Loading tags…</p>
  {:else if !tags.length}
    <p class="mut tag-empty">No tags in this repository.</p>
  {:else}
    <ul class="tag-list" aria-label="{plural(tags.length, 'tag')} in {name}">
      {#each shown as tag (tag.name)}
        <li class="tag-entry">
          <span class="tag-title mono" title={tag.subject ?? undefined}>{tag.name}</span>
          <span class="tag-kind mut">{tag.annotated ? 'annotated' : 'lightweight'}</span>
          <span class="mono mut">{shortId(tag.commit)}</span>
          <span class="grow"></span>
          <button class="btn small" disabled={!!reason} title={reason ?? `Delete ${tag.name} locally or from the remote`} aria-label="Delete tag {tag.name}"
            onclick={event => tagFlow.openDeleteFor([target], tag.name, event.currentTarget)}>Delete…</button>
        </li>
      {/each}
    </ul>
    {#if sorted.length > shown.length}<button class="btn small tag-more" onclick={() => (showAll = true)}>Show all {sorted.length}</button>{/if}
  {/if}
</section>
