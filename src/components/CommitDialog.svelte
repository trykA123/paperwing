<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type ChangeContent, type ChangeFile, type DiffArea, type RepoChanges } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import { dialogOut } from '../lib/motion';
  import Icon from './Icon.svelte';
  import Alert from './Alert.svelte';
  import DiffViewer from './DiffViewer.svelte';

  let { request }: { request: NonNullable<typeof app.gitDialog> } = $props();
  let dialog: HTMLDialogElement;
  let changes = $state<RepoChanges | null>(null);
  let selected = $state<{ path: string; area: DiffArea } | null>(null);
  let content = $state<ChangeContent | null>(null);
  let inline = $state(false);
  let wide = $state(localStorage.getItem('paperwing.commitMax') === '1');
  $effect(() => { localStorage.setItem('paperwing.commitMax', wide ? '1' : '0'); });
  let diffError = $state('');
  let message = $state('');
  let busy = $state(false), loading = $state(true), error = $state('');
  let refreshSeq = 0, diffSeq = 0;

  const staged = $derived(changes?.files.filter(file => file.kind !== 'unmerged' && file.kind !== 'untracked' && file.index !== '.') ?? []);
  const unstaged = $derived(changes?.files.filter(file => file.kind === 'untracked' || file.worktree !== '.') ?? []);
  const areaOf = (file: ChangeFile): DiffArea => (file.kind === 'untracked' ? 'untracked' : 'unstaged');
  const summaryLine = $derived(message.split('\n')[0] ?? '');
  const ready = $derived(!busy && staged.length > 0 && !!message.trim() && !changes?.authorError);

  const LETTERS: Record<string, string> = { M: 'Modified', A: 'Added', D: 'Deleted', R: 'Renamed', C: 'Copied', T: 'Type changed', U: 'Conflict', '?': 'Untracked' };
  const letter = (file: ChangeFile, area: 'staged' | 'unstaged') => (area === 'staged' ? file.index : file.kind === 'untracked' ? '?' : file.worktree);
  const split = (path: string) => { const at = path.lastIndexOf('/'); return at < 0 ? ['', path] : [path.slice(0, at + 1), path.slice(at + 1)]; };

  async function loadDiff() {
    const target = selected;
    const file = target && changes?.files.find(item => item.path === target.path);
    if (!target || !file) { content = null; diffError = ''; return; }
    const id = ++diffSeq;
    try {
      const result = await api.changeContent(request.path, file.path, file.origPath, target.area);
      if (id === diffSeq) { content = result; diffError = ''; }
    } catch (reason) { if (id === diffSeq) { content = null; diffError = String(reason); } }
  }

  function keepSelection() {
    const present = (path: string, area: DiffArea) => (area === 'staged' ? staged : unstaged).some(file => file.path === path && (area === 'staged' || areaOf(file) === area));
    if (selected && present(selected.path, selected.area)) return;
    const first = staged[0] ? { path: staged[0].path, area: 'staged' as const } : unstaged[0] ? { path: unstaged[0].path, area: areaOf(unstaged[0]) } : null;
    selected = first;
  }

  async function refresh() {
    const id = ++refreshSeq;
    try {
      const result = await api.repoChanges(request.path);
      if (id !== refreshSeq) return;
      changes = result;
      error = '';
      keepSelection();
      await loadDiff();
    } catch (reason) { if (id === refreshSeq) error = String(reason); }
    finally { if (id === refreshSeq) loading = false; }
  }

  async function mutate(task: () => Promise<unknown>) {
    if (busy) return;
    busy = true; error = '';
    try { await task(); } catch (reason) { error = String(reason); }
    await refresh();
    busy = false;
  }

  const stage = (files: ChangeFile[]) => mutate(() => api.stagePaths(request.path, files.map(file => file.path)));
  const unstage = (files: ChangeFile[]) => mutate(() => api.unstagePaths(request.path, files.flatMap(file => file.origPath ? [file.path, file.origPath] : [file.path])));

  function select(file: ChangeFile, area: 'staged' | 'unstaged') {
    selected = { path: file.path, area: area === 'staged' ? 'staged' : areaOf(file) };
    void loadDiff();
  }

  async function commit() {
    if (!ready) return;
    busy = true; error = '';
    try {
      const result = await api.commitStaged(request.path, message);
      app.toast(`Committed ${result.sha} \u00b7 ${result.subject}`, 'success', { label: 'Push', run: () => void app.pushRepos([{ path: request.path, name: request.name }]) });
      message = '';
      await app.checkExists([request.path]);
    } catch (reason) { error = String(reason); }
    await refresh();
    busy = false;
    if (changes && !changes.files.length) close();
  }

  function close() {
    if (busy) return;
    void app.checkExists([request.path]);
    app.gitDialog = null;
  }

  onMount(() => {
    dialog.showModal();
    void refresh();
  });
</script>

<dialog class="operation-dialog commit-dialog" class:max={wide} bind:this={dialog} out:dialogOut|global aria-label="Commit changes"
  oncancel={event => { event.preventDefault(); close(); }}>
  <header>
    <h2>Commit changes</h2>
    <span class="mut">{request.name}</span>
    {#if changes}<span class="commit-branch" title={changes.detached ? 'HEAD is detached' : 'Current branch'}><Icon name="branch" tone="branch" />{changes.branch ?? (changes.detached ? `detached @ ${changes.head}` : 'no commits yet')}</span>{/if}
    <span class="grow"></span>
    <button class="icon" title={wide ? 'Restore size' : 'Fill the window'} aria-label={wide ? 'Restore size' : 'Fill the window'} onclick={() => (wide = !wide)}><Icon name={wide ? 'restore' : 'maximize'} /></button>
    <button class="icon" title="Close" aria-label="Close" disabled={busy} onclick={close}><Icon name="close" /></button>
  </header>

  {#if error}<Alert kind="err" role="alert">{error}</Alert>{/if}
  {#if changes?.authorError}<Alert kind="warn">Git cannot tell who you are: {changes.authorError} Set <code>user.name</code> and <code>user.email</code> and reopen this dialog.</Alert>{/if}
  {#if changes?.detached}<Alert kind="warn">HEAD is detached. The commit will not belong to any branch; create a branch first to keep it.</Alert>{/if}

  <div class="commit-body">
    <div class="commit-files">
      {#if loading}<p class="mut commit-empty"><span class="spin"></span> Reading changes…</p>
      {:else if changes && !changes.files.length}<p class="mut commit-empty">No changes. The working tree is clean.</p>
      {:else}
        {#each [{ title: 'Staged', area: 'staged' as const, files: staged }, { title: 'Changes', area: 'unstaged' as const, files: unstaged }] as group (group.area)}
          <section class="commit-group">
            <h3>
              <span>{group.title} <small>{group.files.length}</small></span>
              <span class="grow"></span>
              {#if group.files.length}
                <button class="link" disabled={busy} onclick={() => (group.area === 'staged' ? unstage(group.files) : stage(group.files))}>{group.area === 'staged' ? 'Unstage all' : 'Stage all'}</button>
              {/if}
            </h3>
            {#each group.files as file (file.path + group.area)}
              {@const mark = letter(file, group.area)}
              {@const [dir, name] = split(file.path)}
              <div class="cf" class:sel={selected?.path === file.path && (group.area === 'staged' ? selected.area === 'staged' : selected.area !== 'staged')}
                role="button" tabindex="0" onclick={() => select(file, group.area)}
                onkeydown={event => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); select(file, group.area); } }}>
                <input type="checkbox" checked={group.area === 'staged'} disabled={busy} aria-label="{group.area === 'staged' ? 'Unstage' : 'Stage'} {file.path}"
                  onclick={event => event.stopPropagation()} onchange={() => (group.area === 'staged' ? unstage([file]) : stage([file]))} />
                <span class="cf-mark m-{mark === '?' ? 'q' : mark}" title={LETTERS[mark] ?? mark}>{mark}</span>
                <span class="cf-name" title={file.origPath ? `${file.origPath} \u2192 ${file.path}` : file.path}><span class="mut">{dir}</span><b>{name}</b></span>
                {#if group.area === 'staged' && file.stagedAdded !== null}<span class="cf-stat"><i class="okc">+{file.stagedAdded}</i><i class="err">−{file.stagedRemoved ?? 0}</i></span>{/if}
              </div>
            {/each}
          </section>
        {/each}
        {#if changes?.truncated}<p class="hint commit-empty">Only the first 2000 changed files are listed.</p>{/if}
        {#if changes?.skipped}<p class="hint commit-empty">{changes.skipped} file(s) with non-UTF-8 names are not shown.</p>{/if}
      {/if}
    </div>

    <div class="commit-diff">
      {#if selected}
        <div class="commit-diff-head">
          <span class="mono" title={selected.path}>{selected.path}</span>
          {#if content}<span class="mut">{content.originalLabel} → {content.modifiedLabel}</span>{/if}
          <div class="seg small"><button class:on={!inline} onclick={() => (inline = false)}>Side by side</button><button class:on={inline} onclick={() => (inline = true)}>Inline</button></div>
        </div>
        {#if diffError}<p class="warn commit-empty">{diffError}</p>
        {:else if content?.binary}<p class="mut commit-empty">Binary file; there is no text to compare.</p>
        {:else if content}<DiffViewer original={content.original} modified={content.modified} path={selected.path} {inline} />
        {:else}<p class="mut commit-empty"><span class="spin"></span> Loading…</p>{/if}
      {:else if !loading}<p class="mut commit-empty">Select a file to compare its changes.</p>{/if}
    </div>
  </div>

  <div class="commit-compose">
    <div class="commit-summary" aria-live="polite">
      {#if changes && staged.length}
        <b>{staged.length} file{staged.length === 1 ? '' : 's'} will be committed</b>
        <span class="okc">+{changes.stagedAdded}</span><span class="err">−{changes.stagedRemoved}</span>
        <span class="mut">to {changes.branch ?? 'a detached HEAD'}{changes.author ? ` as ${changes.author}` : ''}</span>
      {:else}<span class="mut">Nothing is staged. Tick files on the left to include them in this commit.</span>{/if}
    </div>
    <textarea rows="3" placeholder="Commit message — the first line is the summary" bind:value={message} spellcheck="true" disabled={busy}
      onkeydown={event => { if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) { event.preventDefault(); void commit(); } }}></textarea>
    <footer>
      <span class="hint" class:warn={summaryLine.length > 72}>{summaryLine.length > 72 ? `Summary is ${summaryLine.length} characters; 72 or fewer is easier to read.` : 'Commit only; nothing is pushed.'}</span>
      <span class="grow"></span>
      <button class="btn" disabled={busy} onclick={close}>Close</button>
      <button class="btn dark" disabled={!ready} title={ready ? 'Ctrl+Enter' : staged.length ? 'Write a commit message' : 'Stage at least one file'} onclick={commit}>
        {#if busy}<span class="spin"></span>{:else}<Icon name="check" />{/if} Commit {staged.length || ''}
      </button>
    </footer>
  </div>
</dialog>
