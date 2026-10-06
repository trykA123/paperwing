<script lang="ts" module>
  export type BulkHandlers = {
    fetch: () => void; pull: () => void; push: () => void; switch: () => void; ref: (anchor: HTMLElement) => void;
    check: () => void; branch: () => void; commit: () => void; clear: () => void;
  };
</script>

<script lang="ts">
  import type { BulkTargets } from '../../lib/formation';
  import type { SetItem } from '../../lib/api';
  import Icon from '../Icon.svelte';

  let { count, targets, dirty, busy, checking, refEligible, handlers }: {
    count: number; refEligible: number; targets: BulkTargets<SetItem>; dirty: number; busy: boolean; checking: boolean; handlers: BulkHandlers;
  } = $props();

  const reason = (empty: string) => (busy ? 'A Git operation is already running' : empty);
</script>

<div class="fm-bar" class:on={count > 0} role="toolbar" aria-label="Selected repositories" inert={count === 0}>
  <span class="count" aria-live="polite">{count} selected</span>
  <button disabled={busy || !targets.fetchable.length} title={reason('None of the selected repositories can be fetched')} onclick={handlers.fetch}>
    <Icon name="refresh" /><span class="t">Fetch</span>{#if targets.fetchable.length}<small>{targets.fetchable.length}</small>{/if}
  </button>
  <button disabled={busy || !targets.behind.length} title={reason('No selected repository is behind its upstream')} onclick={handlers.pull}>
    <Icon name="download" /><span class="t">Pull</span>{#if targets.behind.length}<small>{targets.behind.length}</small>{/if}
  </button>
  <button disabled={busy || !targets.pushable.length} title={reason('Nothing to push in the selected repositories')} onclick={handlers.push}>
    <Icon name="upload" /><span class="t">Push</span>{#if targets.pushable.length}<small>{targets.pushable.length}</small>{/if}
  </button>
  <button disabled={busy || !targets.offRef.length} title={reason('Every selected repository is already on its branch')} onclick={handlers.switch}>
    <Icon name="branch" /><span class="t">Switch</span>{#if targets.offRef.length}<small>{targets.offRef.length}</small>{/if}
  </button>
  <button disabled={busy || !refEligible} title={reason(refEligible ? 'Choose the branch, tag or commit for the selected repositories' : 'Folders opened in place have no remote ref to choose')} onclick={event => handlers.ref(event.currentTarget)}><Icon name="tag" /><span class="sr-only">Ref…</span></button>
  <button disabled={checking} title="Ask each remote which branches and tags exist" onclick={handlers.check}>
    {#if checking}<span class="spin"></span>{:else}<Icon name="search" />{/if}<span class="sr-only">Check refs</span>
  </button>
  <button aria-label="New branch" disabled={busy || !targets.cloned.length} title={reason('New branch in the selected repositories')} onclick={handlers.branch}><Icon name="plus" /><span class="sr-only">New branch</span></button>
  <button disabled={busy || !dirty} title={reason(dirty ? 'Commit the first selected repository with changes' : 'No selected repository has changes')} onclick={handlers.commit}>
    <Icon name="check" /><span class="t">Commit</span>{#if dirty > 1}<small>{dirty}</small>{/if}
  </button>
  <button class="x" aria-label="Clear selection" title="Clear selection" onclick={handlers.clear}><Icon name="close" /></button>
</div>
