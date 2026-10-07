<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type TagInfo } from '../../lib/api';
  import { confirm } from '../../lib/confirm';
  import { dialogOut } from '../../lib/motion';
  import { plural } from '../../lib/plural';
  import { tagFlow } from '../../lib/tag-flow.svelte';
  import { createTags, moveConfirmMessage, moveRows, planTags, releaseTargets, shortId, type CreateRow, type CreateStatus, type PlanRow, type TagTarget } from '../../lib/tags-set';
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';
  import TagReleases from './TagReleases.svelte';

  let { targets }: { targets: TagTarget[] } = $props();
  const LABEL: Record<CreateStatus, string> = { created: 'Tagged', pushed: 'Tagged and pushed', 'push-failed': 'Tagged, push failed', refused: 'Not tagged, name already used', failed: 'Not tagged' };
  let dialog: HTMLDialogElement;
  let input = $state<HTMLInputElement>();
  let name = $state('');
  let message = $state('');
  let push = $state(false);
  let move = $state(false);
  let remotes = $state<Record<string, string>>({});
  let plan = $state<PlanRow[]>([]);
  let planning = $state(false);
  let phase = $state<'review' | 'running' | 'done'>('review');
  let results = $state<CreateRow[]>([]);
  let releasing = $state(false);
  let sequence = 0;
  let confirming = false;
  let lists: Record<string, Promise<TagInfo[]>> = {};

  const many = $derived(targets.length > 1);
  const tag = $derived(name.trim());
  const remoteOf = (target: TagTarget) => (remotes[target.path] ?? target.remote).trim() || 'origin';
  const rows = $derived(plan.map(row => ({ ...row, remote: remoteOf(row) })));
  const clashes = $derived(moveRows(plan));
  const blocked = $derived(!move && plan.length > 0 && plan.every(row => row.existing || row.error));
  const ok = $derived(results.filter(row => row.status === 'pushed' || row.status === 'created').length);
  const failed = $derived(results.length - ok);
  const releasable = $derived(releaseTargets(results));
  const cached = { listTags: (path: string) => (lists[path] ??= api.listTags(path)), localStatus: (paths: string[]) => api.localStatus(paths) };

  async function refresh() {
    const mine = ++sequence;
    if (!tag) { plan = targets.map(target => ({ ...target, existing: null, error: null })); planning = false; return; }
    planning = true;
    const next = await planTags(targets, tag, cached);
    if (mine === sequence) { plan = next; planning = false; }
  }
  $effect(() => { void tag; void refresh(); });
  $effect(() => { if (!clashes.length) move = false; });

  async function run() {
    if (phase !== 'review' || confirming || !tag || blocked) return;
    if (move && clashes.length) {
      confirming = true;
      const accepted = await confirm(moveConfirmMessage(clashes.map(row => ({ ...row, remote: remoteOf(row) })), tag), { title: 'Move tag', kind: 'warning', okLabel: 'Move tag', destructive: true });
      confirming = false;
      if (!accepted || phase !== 'review') return;
    }
    phase = 'running';
    results = [];
    const request = { name: tag, message, push, move };
    const guarded = await tagFlow.guarded(targets.map(target => target.path), () => createTags(rows, request, api, (_, row) => { results = [...results, row]; }));
    lists = {};
    if (!guarded.ran) { phase = 'review'; app.toast('A Git operation is already running', 'warn'); return; }
    results = guarded.value;
    phase = 'done';
    app.toast(failed ? `Tagged ${ok} of ${plural(results.length, 'repository', 'repositories')}` : `Tagged ${plural(results.length, 'repository', 'repositories')} with ${tag}`, failed ? 'warn' : 'success');
  }

  const close = () => { if (phase !== 'running' && !releasing) tagFlow.close(); };
  onMount(() => {
    dialog.showModal();
    input?.focus();
    return () => tagFlow.restoreFocus();
  });
</script>

<dialog class="operation-dialog tag-dialog" bind:this={dialog} out:dialogOut|global aria-label="New tag" oncancel={event => { event.preventDefault(); close(); }}>
  <header>
    <h2>New tag</h2>
    <span class="mut">{many ? plural(targets.length, 'repository', 'repositories') : targets[0].name}</span>
    <span class="grow"></span>
    <button class="icon" title="Close" aria-label="Close" disabled={phase === 'running' || releasing} onclick={close}><Icon name="close" /></button>
  </header>

  <form class="tag-form" onsubmit={event => { event.preventDefault(); void run(); }}>
    {#if phase === 'review'}
      <label class="fld"><span>Tag name</span>
        <input bind:this={input} bind:value={name} class="mono" placeholder="v2.4.0" spellcheck="false" autocomplete="off" aria-describedby="tag-note" />
      </label>
      <label class="fld"><span>Message <em class="mut">optional</em></span>
        <textarea bind:value={message} rows="2" placeholder="What this release is" spellcheck="false"></textarea>
      </label>
      <p id="tag-note" class="tag-note mut">{message.trim() ? 'With a message the tag is annotated.' : 'Without a message the tag is lightweight.'} It points at each repository’s current commit.</p>
      <div class="tag-options">
        <label class="check"><input type="checkbox" bind:checked={push} /> Push after creating</label>
        {#if clashes.length}<label class="check"><input type="checkbox" bind:checked={move} /> Move existing tags</label>{/if}
      </div>
    {:else}
      <p class="tag-lead" role="status" aria-live="polite">
        {#if phase === 'running'}<span class="spin"></span> Tagging {Math.min(results.length + 1, targets.length)} of {targets.length}…
        {:else if failed}<span>Tagged {ok} of {plural(results.length, 'repository', 'repositories')} with <span class="mono">{tag}</span>.</span>
        {:else}<span>Tagged {plural(results.length, 'repository', 'repositories')} with <span class="mono">{tag}</span>.</span>{/if}
      </p>
    {/if}

    <ul class="tag-targets" aria-label={phase === 'review' ? 'Repositories to tag' : 'Results'}>
      {#each rows as row, index (row.path)}
        {@const result = results[index]}
        <li>
          <Icon name={result ? (result.status === 'failed' || result.status === 'refused' || result.status === 'push-failed' ? 'error' : 'check') : 'folder'} size={12} tone={result ? (result.status === 'pushed' || result.status === 'created' ? 'ok' : 'err') : 'repo'} />
          <span class="grow tag-name">{row.name}</span>
          {#if phase === 'review'}
            <span class="mono mut" title="Commit that gets the tag">{shortId(row.commit)}</span>
            {#if row.error}<span class="err tag-badge">Could not read tags</span>
            {:else if row.existing}<span class="tag-badge warn">{move ? `Moves from ${shortId(row.existing.commit)}` : `Refused: tag exists at ${shortId(row.existing.commit)}`}</span>{/if}
            {#if push}<input class="tag-remote mono" aria-label="Remote for {row.name}" value={remoteOf(row)} oninput={event => (remotes[row.path] = event.currentTarget.value)} spellcheck="false" autocomplete="off" />{/if}
          {:else if result}
            <span class={result.status === 'pushed' || result.status === 'created' ? 'okc' : 'err'}>{LABEL[result.status]}</span>
            {#if result.pushed}<span class="mono mut">{result.pushed.remote}</span>{/if}
          {:else if index === results.length && phase === 'running'}<span class="spin"></span>
          {:else}<span class="mut">Waiting</span>{/if}
          {#if result?.error}<p class="tag-error err" role="alert">{result.error}</p>
          {:else if phase === 'review' && row.error}<p class="tag-error err">{row.error}</p>{/if}
        </li>
      {/each}
    </ul>

    {#if phase === 'done' && releasable.length}
      <TagReleases rows={releasable} {tag} message={message.trim()} onbusy={busy => (releasing = busy)} />
    {/if}

    <footer>
      {#if phase === 'review'}
        <span class="hint">{blocked ? 'Every repository already has this tag. Tick Move existing tags to replace it.' : planning ? 'Checking existing tags…' : 'Nothing runs until you choose Create.'}</span>
        <button type="button" class="btn" onclick={close}>Cancel</button>
        <button type="submit" class="btn dark" disabled={!tag || blocked || planning}><Icon name="tag" /> {many ? `Tag ${plural(targets.length, 'repository', 'repositories')}` : 'Create tag'}</button>
      {:else}
        <button type="button" class="btn dark" disabled={phase === 'running' || releasing} onclick={close}>Done</button>
      {/if}
    </footer>
  </form>
</dialog>
