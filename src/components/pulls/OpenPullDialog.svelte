<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type CreatedPullRequest, type SetItem } from '../../lib/api';
  import { redact } from '../../lib/errors';
  import { dialogOut } from '../../lib/motion';
  import { preparePull } from '../../lib/pull-defaults';
  import { forkStatus, pullFlow, pullKey } from '../../lib/pull-flow.svelte';
  import { rateLimitText, readPullsError } from '../../lib/pull-support';
  import { createPull, openPull, pulls } from '../../lib/pulls.svelte';
  import { withBusy } from '../../lib/stash-switch';
  import { app } from '../../lib/state.svelte';
  import Alert from '../Alert.svelte';
  import Icon from '../Icon.svelte';
  import Select from '../Select.svelte';

  let { item }: { item: SetItem } = $props();
  let dialog: HTMLDialogElement;
  let titleInput = $state<HTMLInputElement>();
  let title = $state(''), body = $state(''), base = $state(''), draft = $state(true), pushFirst = $state(false);
  let names = $state<string[]>([]);
  let loading = $state(true), busy = $state(false);
  let error = $state<string | null>(null);
  let created = $state<CreatedPullRequest | null>(null);

  const key = $derived(pullKey(item));
  const local = $derived(key ? app.local[key.path] : undefined);
  const head = $derived(key?.branch ?? '');
  const published = $derived(!!local?.upstream && (local?.ahead ?? 0) === 0);
  const notPushed = $derived(local?.upstream ? `${local.ahead} unpushed ${local.ahead === 1 ? 'commit' : 'commits'}` : 'The branch is not on the remote');
  const known = $derived.by(() => { const entry = key ? pulls.entry(key) : undefined; return entry?.status === 'ready' ? entry.pull : null; });
  const existing = $derived(known && (known.state === 'open' || known.state === 'draft') ? known : null);
  const unpublished = $derived(!local?.upstream);
  const problem = $derived(!title.trim() ? 'Give the pull request a title.' : !base ? 'Choose the branch to merge into.' : forkStatus(item) === 'not-fork' && head === base ? `${head} is the base branch. Check out another branch.` : unpublished && !pushFirst ? 'The branch is not on the remote. Tick “Push first”.' : '');
  const options = $derived(names.map(name => ({ value: name, label: name })));

  async function submit() {
    if (!key || problem || busy || existing) return;
    busy = true; error = null;
    try {
      const guarded = await withBusy(app, async () => {
        if (!published && pushFirst) await api.pushBranch(key.path);
        return createPull(key.path, { head, base, title: title.trim(), body, draft });
      });
      if (!guarded.ran) { error = 'A Git operation is already running. Try again when it finishes.'; return; }
      created = guarded.value;
    } catch (reason) { fail(reason); return; }
    finally { busy = false; }
    pulls.refresh([key]);
    void app.checkExists([key.path]);
    app.toast(`Opened pull request #${created.number}`, 'success');
  }

  function fail(reason: unknown) {
    const issue = readPullsError(reason);
    error = issue.kind === 'rateLimited' ? rateLimitText(issue.resetAt) : redact(issue.message);
  }

  const close = () => { if (!busy) pullFlow.close(); };

  onMount(() => {
    dialog.showModal();
    titleInput?.focus();
    const current = key;
    if (current) {
      void preparePull(current.path, current.branch, { tree: path => app.readTree(path), history: path => api.repositoryHistory(path, 1) }).then(prepared => {
        title = prepared.title; base = prepared.base; names = prepared.names; loading = false;
      });
    }
    return () => pullFlow.restoreFocus();
  });
</script>

<dialog class="operation-dialog stash-dialog pull-dialog" bind:this={dialog} out:dialogOut|global aria-label="Open pull request" oncancel={event => { event.preventDefault(); close(); }}>
  <header>
    <h2>Open pull request</h2>
    <span class="mut">{app.folderOf(item)}</span>
    <span class="grow"></span>
    <button class="icon" title="Close" aria-label="Close" disabled={busy} onclick={close}><Icon name="close" /></button>
  </header>

  {#if created}
    <div class="pull-done" role="status">
      <p class="stash-lead"><Icon name="check" tone="ok" />Opened #{created.number} in <span class="mono">{created.targetRepo}</span></p>
      {#if created.targetRepo.toLowerCase() !== `${item.org}/${item.name}`.toLowerCase()}<Alert kind="info">The pull request went to {created.targetRepo}, not {item.org}/{item.name}.</Alert>{/if}
      {#if created.hasUnpushedCommits}<Alert kind="warn">The branch has commits that are not pushed, so the pull request does not show them yet. Push the branch to include them.</Alert>{/if}
    </div>
    <footer>
      <span class="grow"></span>
      <button class="btn" onclick={close}>Close</button>
      <button class="btn dark" onclick={() => void openPull(created!.url)}><Icon name="remote" /> Open in browser</button>
    </footer>
  {:else}
    <form class="stash-form" onsubmit={event => { event.preventDefault(); void submit(); }}>
      <p class="pull-target">
        <span class="mut">Into</span><span class="mono">{item.org}/{item.name}</span>{#if forkStatus(item) !== 'not-fork'}<span class="mut">or its parent repository</span>{/if}
        <span class="mut">from</span><span class="mono pull-head"><Icon name="branch" size={12} tone="branch" />{head}</span>
      </p>
      {#if existing}<Alert kind="warn">Pull request #{existing.number} is already open for this branch.</Alert>{/if}
      <label class="fld"><span>Title</span><input bind:this={titleInput} bind:value={title} spellcheck="false" autocomplete="off" disabled={busy || loading} required maxlength="256" /></label>
      <label class="fld"><span>Description <em class="mut">optional</em></span><textarea bind:value={body} rows="4" disabled={busy} maxlength="65000"></textarea></label>
      <div class="fld">
        <span id="pull-base">Base on the target repository <em class="mut">(the parent, for forks)</em></span>
        {#if options.length}<Select value={base} {options} label="Base branch" searchable={options.length > 8} disabled={busy} onchange={value => (base = value)} />
        {:else}<input bind:value={base} aria-labelledby="pull-base" spellcheck="false" autocomplete="off" disabled={busy || loading} />{/if}
      </div>
      <label class="check"><input type="checkbox" bind:checked={draft} disabled={busy} /> Open as draft</label>
      {#if !published}
        <Alert kind="warn">{notPushed}. {unpublished ? 'GitHub needs the branch on the remote.' : 'The pull request does not include them.'}</Alert>
        <label class="check"><input type="checkbox" bind:checked={pushFirst} disabled={busy} /> Push first</label>
      {/if}
      {#if error}<p class="stash-note-line err" role="alert">{error}</p>{/if}
      <footer>
        <span class="grow"></span>
        <button type="button" class="btn" disabled={busy} onclick={close}>Cancel</button>
        <button type="submit" class="btn dark" disabled={busy || loading || !!problem || !!existing} title={problem || undefined}>
          {#if busy}<span class="spin"></span>{:else}<Icon name="branch" />{/if} Open pull request
        </button>
      </footer>
    </form>
  {/if}
</dialog>
