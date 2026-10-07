<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type SetItem } from '../../lib/api';
  import { dialogOut } from '../../lib/motion';
  import { plural } from '../../lib/plural';
  import { mapLimit, preparePull } from '../../lib/pull-defaults';
  import { initialRows, openPullRequests, planBulkOpen, type BulkInput, type BulkRow } from '../../lib/pull-bulk';
  import { pullFlow, pullKey } from '../../lib/pull-flow.svelte';
  import { openPull, pulls } from '../../lib/pulls.svelte';
  import { withBusy } from '../../lib/stash-switch';
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';

  let { items }: { items: SetItem[] } = $props();
  let dialog: HTMLDialogElement;
  let phase = $state<'loading' | 'review' | 'running' | 'done'>('loading');
  let inputs = $state<BulkInput[]>([]);
  let rows = $state<BulkRow[]>([]);
  let draft = $state(true), pushFirst = $state(false);

  const plans = $derived(planBulkOpen(inputs, { pushFirst }));
  const runnable = $derived(plans.filter(plan => plan.action !== 'skip'));
  const unpublished = $derived(inputs.some(input => !input.hasRemoteBranch || input.ahead > 0));
  const view = $derived<BulkRow[]>(phase === 'review' || phase === 'loading' ? initialRows(plans) : rows);
  const created = $derived(rows.filter(row => row.result === 'created').length);
  const LABEL = { pending: 'Waiting', skipped: 'Skipped', created: 'Opened', failed: 'Failed', stopped: 'Not attempted' } as const;

  async function load() {
    const reads = { tree: (path: string) => app.readTree(path), history: (path: string) => api.repositoryHistory(path, 1) };
    inputs = await mapLimit(items, 4, async (item): Promise<BulkInput> => {
      const key = pullKey(item)!;
      const local = app.local[key.path];
      const prepared = await preparePull(key.path, key.branch, reads);
      const entry = pulls.entry(key);
      return {
        id: item.id, path: key.path, name: app.folderOf(item), head: key.branch, ahead: local?.ahead ?? 0, hasRemoteBranch: !!local?.upstream,
        base: prepared.base, title: prepared.title, existing: entry?.status === 'ready' ? entry.pull : undefined,
      };
    });
    phase = 'review';
  }

  async function run() {
    if (phase !== 'review' || !runnable.length) return;
    phase = 'running';
    rows = initialRows(plans);
    const guarded = await withBusy(app, () => openPullRequests(plans, api, { draft, pushFirst }, (index, row) => { rows[index] = row; }));
    if (!guarded.ran) { phase = 'review'; app.toast('A Git operation is already running', 'warn'); return; }
    rows = guarded.value;
    phase = 'done';
    pulls.refresh(rows.filter(row => row.result === 'created').map(row => ({ path: row.path, branch: row.head! })));
    void app.checkExists(rows.map(row => row.path));
    const failed = rows.filter(row => row.result === 'failed' || row.result === 'stopped').length;
    app.toast(failed ? `Opened ${created} of ${plural(runnable.length, 'pull request')}` : `Opened ${plural(created, 'pull request')}`, failed ? 'warn' : 'success');
  }

  const close = () => { if (phase !== 'running') pullFlow.close(); };
  onMount(() => {
    dialog.showModal();
    void load();
    return () => pullFlow.restoreFocus();
  });
</script>

<dialog class="operation-dialog stash-dialog pull-dialog pull-bulk" bind:this={dialog} out:dialogOut|global aria-label="Open pull requests" oncancel={event => { event.preventDefault(); close(); }}>
  <header>
    <h2>Open pull requests</h2>
    <span class="mut">{plural(items.length, 'repository', 'repositories')}</span>
    <span class="grow"></span>
    <button class="icon" title="Close" aria-label="Close" disabled={phase === 'running'} onclick={close}><Icon name="close" /></button>
  </header>

  {#if phase === 'loading'}
    <p class="stash-lead" role="status"><span class="spin"></span> Reading the default branch of each repository…</p>
  {:else}
    <p class="stash-lead" role="status" aria-live="polite">
      {#if phase === 'running'}<span class="spin"></span> Opening pull requests…
      {:else if phase === 'done'}Opened {created} of {plural(runnable.length, 'pull request')}.
      {:else}{plural(runnable.length, 'pull request')} to open, {plural(plans.length - runnable.length, 'repository', 'repositories')} skipped. Nothing is pushed unless you choose it.{/if}
    </p>
    <table class="pull-table" aria-label="Pull requests to open">
      <thead><tr><th scope="col">Repository</th><th scope="col">From</th><th scope="col">Into</th><th scope="col">Result</th></tr></thead>
      <tbody>
        {#each view as row (row.id)}
          <tr class:skip={row.result === 'skipped'}>
            <th scope="row" class="pull-repo" title={row.title}>{row.name}</th>
            <td class="mono">{row.head}</td><td class="mono">{row.base || '–'}</td>
            <td class="pull-result">
              {#if row.result === 'created'}<button class="btn small" onclick={() => void openPull(row.created.url)} title={row.created.url}><Icon name="remote" />#{row.created.number}</button>
                {#if row.created.hasUnpushedCommits}<span class="warn">Unpushed commits are not in it</span>{/if}
              {:else if row.result === 'failed' || row.result === 'stopped'}<span class="err" role="alert">{LABEL[row.result]}: {row.message}</span>
              {:else if row.result === 'skipped'}<span class="mut">{LABEL.skipped}: {row.reason}</span>
              {:else}<span class="mut">{phase === 'running' ? LABEL.pending : row.action === 'push-open' ? row.reason : 'Ready'}</span>{/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if phase === 'review'}
      <label class="check"><input type="checkbox" bind:checked={draft} /> Open as draft</label>
      {#if unpublished}<label class="check"><input type="checkbox" bind:checked={pushFirst} /> Push first <span class="mut">(push unpublished branches before opening)</span></label>{/if}
    {/if}
  {/if}

  <footer>
    <span class="grow"></span>
    {#if phase === 'done' || phase === 'running'}
      <button class="btn dark" disabled={phase === 'running'} onclick={close}>Done</button>
    {:else}
      <button class="btn" onclick={close}>Cancel</button>
      <button class="btn dark" disabled={phase === 'loading' || !runnable.length} onclick={run}><Icon name="branch" /> Open {plural(runnable.length, 'pull request')}</button>
    {/if}
  </footer>
</dialog>
