<script lang="ts">
  import { detailsDrawer } from '../../lib/details-drawer.svelte';
  import { api, type StashDiff, type StashEntry } from '../../lib/api';
  import { describeError } from '../../lib/errors';
  import { formatCommitDate } from '../../lib/branch-cleanup';
  import { disabledReason } from '../../lib/menu-reason';
  import { plural } from '../../lib/plural';
  import { stashFlow, pathFacts } from '../../lib/stash-flow.svelte';
  import { restoreStash, type RestoreState } from '../../lib/stash-switch';
  import { app } from '../../lib/state.svelte';
  import Alert from '../Alert.svelte';
  import Icon from '../Icon.svelte';
  import RestoreNote from './RestoreNote.svelte';

  const LINE_CAP = 1500;
  let { path, name, only, drawer = false }: { path: string; name: string; only?: string; drawer?: boolean } = $props();
  let entries = $state<StashEntry[] | null>(null);
  let loadError = $state('');
  let open = $state<string | null>(null);
  let diffs = $state<Record<string, { status: 'loading' } | { status: 'error'; message: string } | { status: 'ready'; diff: StashDiff }>>({});
  let notes = $state<Record<string, RestoreState>>({});
  let busy = $state<string | null>(null);

  const shown = $derived(only ? entries?.filter(entry => entry.oid === only) : entries);
  const facts = $derived(pathFacts(path));
  const stashReason = $derived(disabledReason(['cloned', 'dirty'], facts));
  const idleReason = $derived(disabledReason(['cloned'], facts));
  const lines = (diff: StashDiff) => diff.patch.split('\n');
  const kind = (line: string) => (line.startsWith('+') && !line.startsWith('+++') ? 'add' : line.startsWith('-') && !line.startsWith('---') ? 'del' : line.startsWith('@@') ? 'hunk' : '');
  const title = (entry: StashEntry) => entry.message.replace(/^(WIP )?[Oo]n [^:]+: /, '') || entry.reference;

  async function load() {
    try { entries = await api.stashList(path); loadError = ''; }
    catch (reason) { loadError = describeError(reason, 'list the stashes'); }
  }
  $effect(() => { void stashFlow.revision; void path; void load(); });

  async function preview(entry: StashEntry, event: MouseEvent) {
    if (drawer) { detailsDrawer.open({ kind: 'stash', path, name, oid: entry.oid }, event.currentTarget as Element); return; }
    if (open === entry.oid) { open = null; return; }
    open = entry.oid;
    if (diffs[entry.oid]?.status === 'ready') return;
    diffs[entry.oid] = { status: 'loading' };
    try { diffs[entry.oid] = { status: 'ready', diff: await api.stashShow(path, entry.oid) }; }
    catch (reason) { diffs[entry.oid] = { status: 'error', message: describeError(reason, 'show the stash') }; }
  }

  async function apply(entry: StashEntry) {
    busy = entry.oid;
    notes[entry.oid] = { phase: 'working' };
    const result = await stashFlow.guarded([path], () => restoreStash(path, entry.oid, api));
    if (result.ran) notes[entry.oid] = result.value; else delete notes[entry.oid];
    busy = null;
  }

  async function pop(entry: StashEntry) {
    busy = entry.oid;
    try {
      const result = await stashFlow.guarded([path], () => api.stashPop(path, entry.oid));
      if (!result.ran) return;
      const outcome = result.value;
      if (outcome.applied) { delete notes[entry.oid]; app.toast(`Restored “${title(entry)}” in ${name}`, 'success'); }
      else notes[entry.oid] = outcome.conflicted.length ? { phase: 'conflicted', files: outcome.conflicted, message: outcome.error ?? '' } : { phase: 'failed', message: outcome.error ?? 'The stash did not apply.' };
    } catch (reason) { notes[entry.oid] = { phase: 'failed', message: describeError(reason, 'pop the stash') }; }
    finally { busy = null; }
  }

  async function drop(entry: StashEntry) {
    if (await stashFlow.drop(path, name, entry.oid, title(entry))) app.toast(`Dropped “${title(entry)}”`, 'success');
  }
</script>

<section class="stash-section" aria-label="Stashes">
  <header class="stash-head">
    <h3><Icon name="stash" size={14} tone="record" />Stashes {#if entries}<small>{entries.length}</small>{/if}</h3>
    <button class="btn small" disabled={!!stashReason} title={stashReason ?? 'Set the uncommitted changes aside'}
      onclick={event => stashFlow.openPushFor([{ path, name }], event.currentTarget)}>Stash changes…</button>
  </header>
  {#if loadError}
    <Alert kind="err" role="alert">{loadError}{#snippet action()}<button class="btn small" onclick={load}>Retry</button>{/snippet}</Alert>
  {:else if !entries}
    <p class="mut stash-empty">Loading stashes…</p>
  {:else if !entries.length}
    <p class="mut stash-empty">No stashes in this repository.</p>
  {:else}
    <ul class="stash-list">
      {#each shown ?? [] as entry (entry.oid)}
        <li class="stash-entry" class:open={open === entry.oid}>
          <div class="stash-row">
            <button class="stash-title" aria-expanded={open === entry.oid} title="Preview the changes in this stash" onclick={event => preview(entry, event)}>
              <Icon name="disclosure" size={12} /><span class="stash-msg">{title(entry)}</span>
            </button>
            <span class="stash-meta mut">{#if entry.branch}<span class="mono">{entry.branch}</span> · {/if}{formatCommitDate(entry.createdAt)}</span>
          </div>
          <div class="stash-actions">
            <button class="btn small" disabled={!!idleReason || !!busy} title={idleReason ?? 'Apply and keep the stash'} onclick={() => apply(entry)}>Apply</button>
            <button class="btn small" disabled={!!idleReason || !!busy} title={idleReason ?? 'Apply, then remove the stash if it applied cleanly'} onclick={() => pop(entry)}>Pop</button>
            <button class="btn small" disabled={!!idleReason || !!busy} title={idleReason ?? 'Remove this stash after confirming'} onclick={() => drop(entry)}>Drop…</button>
          </div>
          {#if notes[entry.oid] && notes[entry.oid].phase !== 'working'}
            <RestoreNote state={notes[entry.oid]} {path} ondrop={() => drop(entry)} />
          {/if}
          {#if open === entry.oid}
            {@const diff = diffs[entry.oid]}
            <div class="stash-diff">
              {#if !diff || diff.status === 'loading'}<p class="mut">Loading preview…</p>
              {:else if diff.status === 'error'}<Alert kind="err" role="alert">{diff.message}</Alert>
              {:else}
                {#if diff.diff.notice}<Alert kind="warn" role="status">{diff.diff.notice}. Showing {diff.diff.truncated ? 'the start of the changes' : 'a summary'}.</Alert>{/if}
                {#if diff.diff.hasUntracked}<p class="mut stash-note">Includes untracked files.</p>{/if}
                {@const all = lines(diff.diff)}
                <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
                <pre class="stash-patch mono" tabindex="0" aria-label="Changes in the stash">{#each all.slice(0, LINE_CAP) as line, index (index)}<span class="pl {kind(line)}">{line || ' '}</span>{/each}</pre>
                {#if all.length > LINE_CAP}<p class="mut stash-note">Showing the first {plural(LINE_CAP, 'line')} of {all.length}.</p>{/if}
              {/if}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>
