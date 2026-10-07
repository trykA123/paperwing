<script lang="ts">
  import { repoCounts } from '../../lib/repo-counts';
  import { REPO_SECTIONS, sectionCount, usableSection } from '../../lib/repo-sections';
  import type { View } from '../../lib/workspace';
  import { isCloned } from '../../lib/formation';
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';
  import FavoriteNav from './FavoriteNav.svelte';

  let { view }: { view: Extract<View, { kind: 'repo' }> } = $props();

  const store = app.repositories;
  const entry = $derived(store.resolve(view.repoId));
  const cloned = $derived(!!entry && isCloned(app.local[app.dest(entry.item)]));
  const counts = $derived(entry ? repoCounts(entry) : { changes: 0, branches: null, stash: null, prs: null });
  const locked = $derived(REPO_SECTIONS.filter(section => section.needsClone && !cloned));
  const branch = $derived(entry ? app.local[app.dest(entry.item)]?.branch : null);
</script>

<button class="nav rf-back" onclick={() => store.back()} title="Back to the list (Alt+Left)"><span class="rf-back-i"><Icon name="chevron" size={12} /></span><span class="lbl">Repositories</span></button>

{#if entry}
  <div class="rf-sidehead">
    <b title="{entry.org}/{entry.name}">{app.folderOf(entry.item)}</b>
    <small><Icon name="server" size={12} />{entry.host} / {entry.org}</small>
    {#if cloned && branch}<span class="fm-ref static"><span class="t-branch"><Icon name="branch" /></span><span class="nm">{branch}</span></span>
    {:else if !cloned}<span class="fm-remote">remote only</span>{/if}
  </div>

  <div class="sec">
    <h6>This repository</h6>
    {#each REPO_SECTIONS as section (section.id)}
      {@const off = section.needsClone && !cloned}
      {@const count = off ? null : sectionCount(section.id, counts)}
      <button class="nav" class:on={usableSection(view.section, cloned) === section.id} aria-current={usableSection(view.section, cloned) === section.id ? 'page' : undefined} disabled={off}
        title={off ? 'Needs a clone' : undefined} onclick={() => store.openRepository(view.repoId, section.id)}>
        <Icon name={section.icon} /><span class="lbl">{section.label}</span>
        {#if count !== null && (count > 0 || section.id !== 'changes')}<span class="cnt" class:dirty={section.id === 'changes' && count > 0}>{count}</span>{/if}
      </button>
    {/each}
    {#if locked.length}<p class="sb-note">Needs a clone: {locked.map(section => section.label).join(', ')}.</p>{/if}
  </div>
{/if}

<FavoriteNav current={view.repoId} />
