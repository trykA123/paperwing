<script lang="ts">
  import { api, type ChangeFile, type RepoChanges } from '../../../lib/api';
  import { describeError } from '../../../lib/errors';
  import { plural } from '../../../lib/plural';
  import type { RepoEntry } from '../../../lib/repositories';
  import { stashFlow } from '../../../lib/stash-flow.svelte';
  import { app } from '../../../lib/state.svelte';
  import Alert from '../../Alert.svelte';
  import Icon from '../../Icon.svelte';
  import SectionCard from './SectionCard.svelte';

  let { entry, limit = 0 }: { entry: RepoEntry; limit?: number } = $props();

  const LETTERS: Record<string, string> = { M: 'Modified', A: 'Added', D: 'Deleted', R: 'Renamed', C: 'Copied', T: 'Type changed', U: 'Conflict', '?': 'Untracked' };
  let changes = $state<RepoChanges | null>(null);
  let error = $state('');
  let sequence = 0;
  const path = $derived(app.dest(entry.item));
  const dirty = $derived(app.local[path]?.dirty ?? 0);
  const busy = $derived(app.running || app.gitBusy || app.clonePreparing);
  const staged = $derived(changes?.files.filter(file => file.kind !== 'unmerged' && file.kind !== 'untracked' && file.index !== '.').length ?? 0);
  const files = $derived(limit ? changes?.files.slice(0, limit) ?? [] : changes?.files ?? []);
  const mark = (file: ChangeFile) => (file.kind === 'untracked' ? '?' : file.index !== '.' ? file.index : file.worktree);
  const split = (name: string) => { const at = name.lastIndexOf('/'); return at < 0 ? ['', name] : [name.slice(0, at + 1), name.slice(at + 1)]; };

  async function load() {
    const mine = ++sequence;
    try { const next = await api.repoChanges(path); if (mine === sequence) { changes = next; error = ''; } }
    catch (reason) { if (mine === sequence) error = describeError(reason, 'read the changes'); }
  }

  $effect(() => { void dirty; void app.gitDialog; void stashFlow.revision; void load(); });
</script>

<SectionCard title="Changes" sub={changes ? `${plural(changes.files.length, 'file')} on ${changes.branch ?? 'a detached HEAD'}` : ''}>
  {#snippet actions()}
    <button class="btn small dark" disabled={busy || !dirty} title="Stage files and write the commit message" onclick={() => app.openGitDialog('commit', entry.item)}><Icon name="check" />{dirty ? `Commit ${dirty} files…` : 'Commit…'}</button>
    <button class="btn small" disabled={busy || !dirty} onclick={event => stashFlow.openPush([entry.item], event.currentTarget)}><Icon name="stash" tone="record" />Stash…</button>
  {/snippet}
  {#if error}<Alert kind="err" role="alert">{error}</Alert>
  {:else if !changes}<p class="mut"><span class="spin"></span> Reading changes…</p>
  {:else if !changes.files.length}<p class="mut rf-none">No changes. The working tree is clean.</p>
  {:else}
    <ul class="rf-files" aria-label="Changed files">
      {#each files as file (file.path)}
        {@const letter = mark(file)}
        {@const [dir, name] = split(file.path)}
        <li class="cf"><span class="cf-mark m-{letter === '?' ? 'q' : letter}" title={LETTERS[letter] ?? letter}>{letter}</span><span class="cf-name" title={file.path}><span class="mut">{dir}</span><b>{name}</b></span>
          {#if file.stagedAdded !== null}<span class="cf-stat"><i class="okc">+{file.stagedAdded}</i><i class="err">−{file.stagedRemoved ?? 0}</i></span>{:else if file.index !== '.' && file.kind !== 'untracked'}<span class="mut">staged</span>{/if}</li>
      {/each}
    </ul>
    {#if limit && changes.files.length > limit}<p class="mut rf-none">and {changes.files.length - limit} more</p>{/if}
    <p class="hint rf-none">{staged ? `${plural(staged, 'file')} staged.` : 'Nothing is staged yet.'} Staging and the diff open in the commit window.</p>
  {/if}
</SectionCard>
