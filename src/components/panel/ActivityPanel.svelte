<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';
  import Select, { type SelectOption } from '../Select.svelte';

  const STATUSES: SelectOption[] = [
    { value: 'all', label: 'All' }, { value: 'running', label: 'Running' }, { value: 'completed', label: 'Completed' },
    { value: 'failed', label: 'Failed' }, { value: 'cancelled', label: 'Cancelled' }, { value: 'timedOut', label: 'Timed out' },
  ];

  let query = $state('');
  let filter = $state('all');
  const entries = $derived(app.activity.filter(entry => (filter === 'all' || entry.state === filter)
    && `${entry.context} ${entry.argv.join(' ')}`.toLowerCase().includes(query.toLowerCase())).reverse());

  onMount(() => { app.refreshActivity().catch(error => app.toast(String(error), 'error')); });
</script>

<div class="panel-activity">
  <header class="activity-toolbar">
    <strong><Icon name="activity" /> Git activity <small>{app.activity.length}</small></strong>
    <span class="grow"></span>
    <button class="shell-control" title="Recover activity snapshot" aria-label="Refresh activity" onclick={() => app.refreshActivity().catch(error => app.toast(String(error), 'error'))}><Icon name="refresh" /></button>
    <button class="shell-control" title="Clear finished activity" aria-label="Clear finished activity" disabled={!app.activity.some(entry => entry.state !== 'running')}
      onclick={() => app.clearActivity().catch(error => app.toast(String(error), 'error'))}><Icon name="trash" /></button>
  </header>
  <div class="activity-filters">
    <input aria-label="Filter Git activity" placeholder="Filter commands" bind:value={query} />
    <Select label="Activity status" options={STATUSES} value={filter} onchange={value => (filter = value)} />
  </div>
  <div class="activity-list">
    {#each entries as entry (entry.id)}
      <details class="activity-entry" class:failed={entry.state === 'failed' || entry.state === 'timedOut'}>
        <summary><span class="activity-state">{entry.state}</span><span class="activity-command">git {entry.argv.map(arg => JSON.stringify(arg)).join(' ')}</span><small>{entry.elapsedMs} ms</small></summary>
        <div class="activity-meta"><span>{entry.id} · {entry.context} · {new Date(entry.startedAt).toLocaleTimeString()} · exit {entry.exitCode ?? '-'} · {entry.stdoutBytes}/{entry.stderrBytes} bytes</span>
          {#if entry.state === 'running'}<button class="shell-control" aria-label="Cancel {entry.id}" title="Cancel Git command" onclick={() => api.cancelActivity(entry.id).catch(error => app.toast(String(error), 'error'))}><Icon name="close" /></button>{/if}
        </div>
        {#if entry.truncated}<p class="activity-truncated">Output truncated; bounded redacted preview.</p>{/if}
        <pre>{entry.output.map(line => `${line.sequence} [${line.stream}] ${line.text}`).join('\n') || '(no output)'}</pre>
      </details>
    {:else}<p class="mut activity-empty">No matching activity</p>{/each}
  </div>
</div>
