<script lang="ts">
  import { api, type ChangeFile, type RepoChanges } from '../../lib/api';
  import { describeError } from '../../lib/errors';
  import { stashFlow } from '../../lib/stash-flow.svelte';
  import { app } from '../../lib/state.svelte';
  import Alert from '../Alert.svelte';

  let { path, limit = 0, changes = $bindable(null) }: { path: string; limit?: number; changes?: RepoChanges | null } = $props();

  const LETTERS: Record<string, string> = { M: 'Modified', A: 'Added', D: 'Deleted', R: 'Renamed', C: 'Copied', T: 'Type changed', U: 'Conflict', '?': 'Untracked' };
  let error = $state('');
  let sequence = 0;
  const dirty = $derived(app.local[path]?.dirty ?? 0);
  const files = $derived(limit ? changes?.files.slice(0, limit) ?? [] : changes?.files ?? []);
  const mark = (file: ChangeFile) => (file.kind === 'untracked' ? '?' : file.index !== '.' ? file.index : file.worktree);
  const split = (name: string) => { const at = name.lastIndexOf('/'); return at < 0 ? ['', name] : [name.slice(0, at + 1), name.slice(at + 1)]; };

  async function load() {
    const mine = ++sequence;
    try { const next = await api.repoChanges(path); if (mine === sequence) { changes = next; error = ''; } }
    catch (reason) { if (mine === sequence) error = describeError(reason, 'read the changes'); }
  }

  $effect(() => { void dirty; void app.gitDialog; void stashFlow.revision; void path; void load(); });
</script>

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
  {#if limit && changes.files.length > limit}<p class="mut rf-none">and {changes.files.length - limit} more files</p>{/if}
{/if}
