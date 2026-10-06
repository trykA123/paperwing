<script lang="ts">
  import { untrack } from 'svelte';
  import type { SetItem } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';

  let { item }: { item: SetItem } = $props();

  let open = $state(false);
  const path = $derived(app.dest(item));
  const tree = $derived(app.trees[path]);

  $effect(() => {
    if (!open) return;
    const target = path;
    const consumer = new AbortController();
    untrack(() => { app.openTreePaths = [target]; void app.loadTree(target, false, consumer.signal); });
    return () => { consumer.abort(); app.openTreePaths = []; };
  });

  /** Git names stashes "On <branch>: <message>"; show the message and keep the branch as detail. */
  function stashParts(subject: string) {
    const match = subject.match(/^(?:WIP )?[Oo]n (.+?): ([\s\S]*)$/);
    return match ? { branch: match[1], message: match[2] } : { branch: '', message: subject };
  }
</script>

<div class="tree-block">
  <button class="tree-head" aria-expanded={open} onclick={() => (open = !open)}>
    <span class="tree-chev" class:open><Icon name="disclosure" size={12} /></span>Branches, tags and stashes
  </button>
  {#if open}
    <div class="repo-subtree">
      <button class="tree-refresh" title="Refresh repository tree" aria-label="Refresh tree" disabled={tree?.loading} onclick={() => app.loadTree(path, true)}><Icon name="refresh" /></button>
      {#if tree?.loading}<span class="mut">Loading...</span>{/if}
      {#if tree?.error}<p class="tree-error" role="status">{tree.error}</p>{/if}
      {#if tree?.data}
        <details><summary><Icon name="disclosure" size={12} /><Icon name="branch" size={12} tone="branch" />Branches <small>{tree.data.branches.length}</small></summary>
          {#each tree.data.branches as branch}<div class="tree-entry" title={branch.sha}><Icon name="branch" size={12} tone="branch" /><span>{branch.label ?? branch.name}</span>{#if branch.current}<Icon name="check" size={12} tone="ok" />{:else}<button class="tree-delete" title="Delete local branch (the remote is not touched)" aria-label="Delete local branch {branch.label ?? branch.name}" disabled={app.running || app.gitBusy} onclick={() => app.deleteLocalBranch(path, app.folderOf(item), branch.name, branch.label ?? branch.name)}><Icon name="trash" size={12} /></button>{/if}</div>{:else}<span class="mut">None</span>{/each}
        </details>
        <details><summary><Icon name="disclosure" size={12} /><Icon name="remote" size={12} tone="repo" />Remotes <small>{tree.data.remotes.length}</small></summary>
          {#each tree.data.remotes as remote}<details class="remote-tree"><summary><Icon name="disclosure" size={12} /><Icon name="remote" size={12} tone="repo" />{remote.name}</summary>
            {#each remote.urls as url}<div class="tree-url">{url}</div>{/each}
            {#each remote.refs as reference}<div class="tree-entry" title={`${reference.sha} ${reference.symbolic}`}><Icon name="branch" size={12} tone="branch" /><span>{reference.label ?? reference.name}</span></div>{/each}
          </details>{:else}<span class="mut">None</span>{/each}
        </details>
        <details><summary><Icon name="disclosure" size={12} /><Icon name="tag" size={12} tone="tag" />Tags <small>{tree.data.tags.length}</small></summary>
          {#each tree.data.tags as tag}<div class="tree-entry" title={tag.sha}><Icon name="tag" size={12} tone="tag" /><span>{tag.label ?? tag.name}</span></div>{:else}<span class="mut">None</span>{/each}
        </details>
        <details><summary><Icon name="disclosure" size={12} /><Icon name="stash" size={12} tone="record" />Stashes <small>{tree.data.stashes.length}</small></summary>
          {#each tree.data.stashes as stash}{@const parts = stashParts(stash.subject)}<div class="tree-entry" title="{stash.name} · {stash.sha}"><Icon name="stash" size={12} tone="record" /><span>{parts.message}</span><small class="tree-meta">{stash.name}{parts.branch ? ` · ${parts.branch}` : ''}</small></div>{:else}<span class="mut">None</span>{/each}
        </details>
        <details><summary><Icon name="disclosure" size={12} /><Icon name="submodule" size={12} tone="folder" />Submodules <small>{tree.data.submodules.length}</small></summary>
          {#each tree.data.submodules as module}<div class="tree-entry" title={`${module.sha} ${module.url ?? ''}`}><Icon name="folder" size={12} tone="folder" /><span>{module.path}</span></div>{:else}<span class="mut">None</span>{/each}
        </details>
      {/if}
    </div>
  {/if}
</div>
