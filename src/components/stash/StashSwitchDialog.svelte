<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../../lib/api';
  import { dialogOut } from '../../lib/motion';
  import { plural } from '../../lib/plural';
  import { stashFlow } from '../../lib/stash-flow.svelte';
  import { restoreStash, rowStatus, stashedRows, switchWithStash, type RestoreState, type SwitchRow, type SwitchTarget } from '../../lib/stash-switch';
  import { app } from '../../lib/state.svelte';
  import Alert from '../Alert.svelte';
  import Icon from '../Icon.svelte';
  import RestoreNote from './RestoreNote.svelte';

  let { targets }: { targets: SwitchTarget[] } = $props();
  let dialog: HTMLDialogElement;
  let primary = $state<HTMLButtonElement>();
  let phase = $state<'review' | 'running' | 'done'>('review');
  let rows = $state<SwitchRow[]>([]);
  let restores = $state<Record<string, RestoreState>>({});
  let current = $state(0);

  const stashed = $derived(stashedRows(rows));
  const switched = $derived(rows.filter(row => row.switched).length);
  const pending = $derived(stashed.filter(row => !restores[row.path] || restores[row.path].phase === 'idle'));
  const working = $derived(Object.values(restores).some(state => state.phase === 'working'));
  const LABEL = { switched: 'Switched', 'switched-stashed': 'Switched, changes stashed', failed: 'Not switched', 'failed-stashed': 'Not switched, changes stashed' } as const;
  const dirtyNow = (path: string) => app.local[path]?.dirty ?? 0;
  const oldBranch = (path: string) => app.local[path]?.branch ?? 'detached HEAD';
  const message = (row: SwitchRow) => `Skein: before switching to ${row.branch}`;

  async function run() {
    if (!await app.guardBuffers()) return;
    phase = 'running';
    app.gitBusy = true;
    try {
      rows = await switchWithStash(targets, api, index => { current = index + 1; });
    } finally {
      app.gitBusy = false;
      phase = 'done';
      stashFlow.changed();
      await app.checkExists(targets.map(target => target.path));
    }
    const failed = rows.length - switched;
    app.toast(failed ? `Switched ${switched} of ${plural(rows.length, 'repository', 'repositories')}` : `Switched ${plural(rows.length, 'repository', 'repositories')}`, failed ? 'warn' : 'success');
  }

  async function restore(row: SwitchRow) {
    if (!row.stashed) return;
    restores[row.path] = { phase: 'working' };
    restores[row.path] = await stashFlow.guarded([row.path], () => restoreStash(row.path, row.stashed!, api));
  }

  async function restoreAll() { for (const row of pending) await restore(row); }

  async function drop(row: SwitchRow) {
    if (row.stashed && await stashFlow.drop(row.path, row.name, row.stashed, message(row))) restores[row.path] = { phase: 'dropped' };
  }

  const close = () => { if (phase !== 'running' && !working) stashFlow.close(); };
  onMount(() => { dialog.showModal(); primary?.focus(); });
</script>

<dialog class="operation-dialog stash-dialog" bind:this={dialog} out:dialogOut|global aria-label="Switch with stash" oncancel={event => { event.preventDefault(); close(); }}>
  <header>
    <h2>Switch with stash</h2>
    <span class="mut">{plural(targets.length, 'repository', 'repositories')}</span>
    <span class="grow"></span>
    <button class="icon" title="Close" aria-label="Close" disabled={phase === 'running' || working} onclick={close}><Icon name="close" /></button>
  </header>

  {#if phase === 'review'}
    <p class="stash-lead">Uncommitted changes, including untracked files, are stashed first. Then each repository switches to the set’s branch, one at a time. Afterwards you choose which stashes to restore. Nothing is dropped.</p>
    <ul class="stash-targets" aria-label="Repositories to switch">
      {#each targets as target (target.path)}
        <li><Icon name="folder" size={12} tone="repo" /><span class="grow stash-name">{target.name}</span>
          <span class="mono mut">{oldBranch(target.path)}</span><Icon name="chevron" size={12} /><span class="mono">{target.branch}</span>
          {#if dirtyNow(target.path)}<span class="stash-tag warn">{plural(dirtyNow(target.path), 'change')} stashed</span>{/if}</li>
      {/each}
    </ul>
  {:else}
    <p class="stash-lead" role="status" aria-live="polite">
      {#if phase === 'running'}<span class="spin"></span> Switching {current} of {targets.length}…
      {:else}Switched {switched} of {plural(rows.length, 'repository', 'repositories')}.{/if}
    </p>
    <ul class="stash-targets" aria-label="Results">
      {#each targets as target, index (target.path)}
        {@const row = rows[index]}
        <li class="stash-result">
          <Icon name={row ? (row.switched ? 'check' : 'error') : 'branch'} size={12} tone={row ? (row.switched ? 'ok' : 'err') : 'branch'} />
          <span class="grow stash-name"><span>{target.name}</span><Icon name="chevron" size={12} /><span class="mono mut">{target.branch}</span></span>
          {#if row}<span class={row.switched ? 'okc' : 'err'}>{LABEL[rowStatus(row)]}</span>{:else if index < current}<span class="spin"></span>{:else}<span class="mut">Waiting</span>{/if}
          {#if row?.error}<p class="stash-error err" role="alert">{row.error}</p>{/if}
        </li>
      {/each}
    </ul>

    {#if phase === 'done' && stashed.length}
      <section class="stash-restore" aria-label="Restore stashes">
        <header class="stash-restore-head">
          <h3>Restore stashes</h3>
          <span class="grow"></span>
          <button class="btn small" disabled={!pending.length || working} onclick={restoreAll}>Restore all{pending.length > 1 ? ` (${pending.length})` : ''}</button>
        </header>
        <p class="mut stash-note">Restoring applies the stash and keeps it. If it conflicts, the stash stays and the conflicting files are listed.</p>
        <ul class="stash-targets">
          {#each stashed as row (row.path)}
            {@const state = restores[row.path] ?? { phase: 'idle' }}
            <li class="stash-result">
              <Icon name="stash" size={12} tone="record" />
              <span class="grow stash-name"><span>{row.name}</span><span class="mut">{row.switched ? `on ${row.branch}` : `still on ${oldBranch(row.path)}`}</span></span>
              <button class="btn small" disabled={working || state.phase === 'applied' || state.phase === 'dropped'} onclick={() => restore(row)}>
                {#if state.phase === 'working'}<span class="spin"></span>{/if}{state.phase === 'conflicted' || state.phase === 'failed' ? 'Try again' : 'Restore'}
              </button>
              {#if state.phase !== 'idle' && state.phase !== 'working'}<div class="stash-note-row"><RestoreNote {state} path={row.path} ondrop={() => drop(row)} /></div>{/if}
            </li>
          {/each}
        </ul>
        {#if pending.length}<Alert kind="info" role="status">Stashes you leave alone stay in each repository’s Stashes list in History.</Alert>{/if}
      </section>
    {/if}
  {/if}

  <footer>
    <span class="grow"></span>
    {#if phase === 'review'}
      <button class="btn" onclick={close}>Cancel</button>
      <button class="btn dark" bind:this={primary} onclick={run}><Icon name="branch" /> Switch {plural(targets.length, 'repository', 'repositories')}</button>
    {:else}
      <button class="btn dark" disabled={phase === 'running' || working} onclick={close}>Done</button>
    {/if}
  </footer>
</dialog>
