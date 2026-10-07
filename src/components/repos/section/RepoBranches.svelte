<script lang="ts">
  import { untrack } from 'svelte';
  import { isCloned } from '../../../lib/formation';
  import { plural } from '../../../lib/plural';
  import type { RepoEntry } from '../../../lib/repositories';
  import { app } from '../../../lib/state.svelte';
  import Icon from '../../Icon.svelte';
  import TagSection from '../../tags/TagSection.svelte';
  import SectionCard from './SectionCard.svelte';

  let { entry }: { entry: RepoEntry } = $props();

  const path = $derived(app.dest(entry.item));
  const tree = $derived(app.trees[path]);
  const remote = $derived(app.refs[entry.url]);
  const cloned = $derived(isCloned(app.local[path]));
  const busy = $derived(app.running || app.gitBusy || app.clonePreparing);

  $effect(() => { if (!cloned) { const url = entry.url; untrack(() => void app.ensureRefs([url])); } });
</script>

{#if !cloned}
  <SectionCard title="Branches on {entry.host}" sub="Read only until the repository is cloned">
    {#if !remote}<p class="mut"><span class="spin"></span> Asking {entry.host}…</p>
    {:else if remote.error}<p class="warn" role="status">{remote.error}</p>
    {:else}
      <ul class="rf-refs">
        {#each remote.branches as name (name)}<li><Icon name="branch" tone="branch" /><span class="mono">{name}</span></li>{/each}
        {#each remote.tags as name (name)}<li><Icon name="tag" tone="tag" /><span class="mono">{name}</span></li>{/each}
      </ul>
    {/if}
  </SectionCard>
{:else}
  <SectionCard title="Branches" sub={tree?.data ? plural(tree.data.branches.length, 'local branch', 'local branches') : ''}>
    {#snippet actions()}
      <button class="btn small" disabled={busy} title="Delete the branches that are already merged" onclick={() => app.openCleanupDialog([entry.item])}><Icon name="trash" tone="branch" />Clean up merged…</button>
    {/snippet}
    {#if tree?.loading && !tree.data}<p class="mut"><span class="spin"></span> Reading branches…</p>
    {:else if tree?.error}<p class="warn" role="status">{tree.error}</p>
    {:else if tree?.data}
      <ul class="rf-refs">
        {#each tree.data.branches as branch (branch.name)}
          <li><Icon name="branch" tone="branch" /><span class="mono">{branch.label ?? branch.name}</span>
            {#if branch.current}<span class="fm-ondisk"><Icon name="check" size={11} tone="ok" />checked out</span>
            {:else}<span class="grow"></span><button class="btn small" disabled={busy} title="Delete the local branch. Nothing on {entry.host} changes." aria-label="Delete local branch {branch.label ?? branch.name}" onclick={() => app.deleteLocalBranch(path, app.folderOf(entry.item), branch.name, branch.label ?? branch.name)}><Icon name="trash" tone="danger" />Delete…</button>{/if}
          </li>
        {/each}
      </ul>
    {/if}
  </SectionCard>
  <SectionCard label="Tags"><TagSection {path} name={app.folderOf(entry.item)} /></SectionCard>
{/if}
