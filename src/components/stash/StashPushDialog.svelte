<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../../lib/api';
  import { describeError } from '../../lib/errors';
  import { dialogOut } from '../../lib/motion';
  import { plural } from '../../lib/plural';
  import { stashFlow, type StashTarget } from '../../lib/stash-flow.svelte';
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';

  let { targets }: { targets: StashTarget[] } = $props();
  type Result = { state: 'stashed' | 'nothing' | 'failed'; message?: string };
  let dialog: HTMLDialogElement;
  let input: HTMLInputElement;
  let message = $state('');
  let untracked = $state(false);
  let busy = $state(false);
  let results = $state<Record<string, Result>>({});

  const many = $derived(targets.length > 1);
  const finished = $derived(targets.every(target => results[target.path]));
  const anyNothing = $derived(targets.some(target => results[target.path]?.state === 'nothing'));
  const stashedCount = $derived(Object.values(results).filter(result => result.state === 'stashed').length);

  async function run() {
    if (busy) return;
    busy = true;
    const guarded = await stashFlow.guarded(targets.map(target => target.path), async () => {
      for (const target of targets) {
        if (results[target.path]?.state === 'stashed') continue;
        try {
          const outcome = await api.stashPush(target.path, message.trim() || null, untracked);
          results[target.path] = { state: outcome.nothingToStash ? 'nothing' : 'stashed' };
        } catch (reason) { results[target.path] = { state: 'failed', message: describeError(reason, `stash ${target.name}`) }; }
      }
    });
    busy = false;
    if (!guarded.ran) { app.toast('A Git operation is already running', 'warn'); return; }
    if (targets.every(target => results[target.path]?.state === 'stashed')) {
      app.toast(many ? `Stashed changes in ${plural(targets.length, 'repository', 'repositories')}` : `Stashed changes in ${targets[0].name}`, 'success');
      stashFlow.close();
    }
  }

  const close = () => { if (!busy) stashFlow.close(); };
  onMount(() => {
    dialog.showModal();
    input.focus();
    return () => stashFlow.restoreFocus();
  });
</script>

<dialog class="operation-dialog stash-dialog" bind:this={dialog} out:dialogOut|global aria-label="Stash changes" oncancel={event => { event.preventDefault(); close(); }}>
  <header>
    <h2>Stash changes</h2>
    <span class="mut">{many ? plural(targets.length, 'repository', 'repositories') : targets[0].name}</span>
    <span class="grow"></span>
    <button class="icon" title="Close" aria-label="Close" disabled={busy} onclick={close}><Icon name="close" /></button>
  </header>
  <form class="stash-form" onsubmit={event => { event.preventDefault(); void run(); }}>
    {#if many}
      <ul class="stash-targets" aria-label="Repositories">
        {#each targets as target (target.path)}
          {@const result = results[target.path]}
          <li><Icon name="folder" size={12} tone="repo" /><span class="grow">{target.name}</span>
            {#if result?.state === 'stashed'}<span class="okc">stashed</span>
            {:else if result?.state === 'nothing'}<span class="mut">nothing to stash</span>
            {:else if result?.state === 'failed'}<span class="err" title={result.message}>{result.message}</span>{/if}</li>
        {/each}
      </ul>
    {:else if results[targets[0].path]?.state === 'nothing'}
      <p class="stash-note-line mut">Nothing to stash. New (untracked) files stay in the folder unless you tick “Include untracked files”.</p>
    {:else if results[targets[0].path]?.state === 'failed'}
      <p class="stash-note-line err" role="alert">{results[targets[0].path].message}</p>
    {/if}
    <label class="fld"><span>Message <em class="mut">optional</em></span>
      <input bind:this={input} bind:value={message} placeholder="What is in this stash" spellcheck="false" autocomplete="off" disabled={busy} />
    </label>
    <label class="check"><input type="checkbox" bind:checked={untracked} disabled={busy} /> Include untracked files</label>
    {#if !untracked && (!many || anyNothing)}<p class="stash-note-line mut">New (untracked) files stay in the folder.</p>{/if}
    <footer>
      <span class="hint">Changes are saved in a stash and the working folder is cleaned. Nothing is dropped.</span>
      <span class="grow"></span>
      <button type="button" class="btn" disabled={busy} onclick={close}>{finished && stashedCount ? 'Close' : 'Cancel'}</button>
      <button type="submit" class="btn dark" disabled={busy}>{#if busy}<span class="spin"></span>{:else}<Icon name="stash" />{/if} {many ? `Stash ${plural(targets.length, 'repository', 'repositories')}` : 'Stash changes'}</button>
    </footer>
  </form>
</dialog>
