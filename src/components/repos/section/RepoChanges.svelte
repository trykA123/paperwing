<script lang="ts">
  import type { RepoChanges } from '../../../lib/api';
  import { plural } from '../../../lib/plural';
  import type { RepoEntry } from '../../../lib/repositories';
  import { stashFlow } from '../../../lib/stash-flow.svelte';
  import { app } from '../../../lib/state.svelte';
  import ChangedFiles from '../ChangedFiles.svelte';
  import Icon from '../../Icon.svelte';
  import SectionCard from './SectionCard.svelte';

  let { entry, limit = 0 }: { entry: RepoEntry; limit?: number } = $props();

  let changes = $state<RepoChanges | null>(null);
  const path = $derived(app.dest(entry.item));
  const dirty = $derived(app.local[path]?.dirty ?? 0);
  const busy = $derived(app.running || app.gitBusy || app.clonePreparing);
  const staged = $derived(changes?.files.filter(file => file.kind !== 'unmerged' && file.kind !== 'untracked' && file.index !== '.').length ?? 0);
</script>

<SectionCard title="Changes" sub={changes ? `${plural(changes.files.length, 'file')} on ${changes.branch ?? 'a detached HEAD'}` : ''}>
  {#snippet actions()}
    <button class="btn small dark" disabled={busy || !dirty} title="Stage files and write the commit message" onclick={() => app.openGitDialog('commit', entry.item)}><Icon name="check" />{dirty ? `Commit ${dirty} files…` : 'Commit…'}</button>
    <button class="btn small" disabled={busy || !dirty} onclick={event => stashFlow.openPush([entry.item], event.currentTarget)}><Icon name="stash" tone="record" />Stash…</button>
  {/snippet}
  <ChangedFiles {path} {limit} bind:changes />
  {#if changes?.files.length}<p class="hint rf-none">{staged ? `${plural(staged, 'file')} staged.` : 'Nothing is staged yet.'} Staging and the diff open in the commit window.</p>{/if}
</SectionCard>
