<script lang="ts" module>
  export type BulkHandlers = {
    fetch: () => void; pull: () => void; push: () => void; switch: () => void; ref: (anchor: HTMLElement) => void;
    check: () => void; branch: () => void; cleanup: () => void; commit: () => void; clear: () => void;
  };
</script>

<script lang="ts">
  import type { BulkTargets } from '../../lib/formation';
  import type { SetItem } from '../../lib/api';
  import Icon from '../Icon.svelte';

  let { count, targets, dirty, busy, checking, refEligible, handlers }: {
    count: number; refEligible: number; targets: BulkTargets<SetItem>; dirty: number; busy: boolean; checking: boolean; handlers: BulkHandlers;
  } = $props();

  let open = $state(false);
  let more: HTMLButtonElement;
  let menu = $state<HTMLDivElement>();
  const reason = (empty: string) => (busy ? 'A Git operation is already running' : empty);
  const refReason = $derived(refEligible ? 'Choose the branch, tag or commit for the selected repositories' : 'Folders opened in place have no remote ref to choose');
  const items = () => [...(menu?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])];

  $effect(() => { if (!count) open = false; });
  $effect(() => { if (open) queueMicrotask(() => items()[0]?.focus()); });

  function run(action: () => void) {
    open = false;
    action();
  }

  function onKey(event: KeyboardEvent) {
    if (!open) return;
    if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); open = false; more.focus(); return; }
    const step = event.key === 'ArrowDown' ? 1 : event.key === 'ArrowUp' ? -1 : 0;
    if (!step) return;
    event.preventDefault();
    const list = items();
    const at = list.indexOf(document.activeElement as HTMLButtonElement);
    list[(at + step + list.length) % list.length]?.focus();
  }
</script>

<svelte:window onpointerdown={event => { if (open && !(event.target as Element).closest('.bulk-more-wrap')) open = false; }} />

<div class="fm-bar" class:on={count > 0} role="toolbar" aria-label="Selected repositories" inert={count === 0}>
  <span class="count" aria-live="polite">{count}<span class="t">&nbsp;selected</span></span>
  <button disabled={busy || !targets.fetchable.length} title={targets.fetchable.length ? `Fetch ${targets.fetchable.length} selected` : reason('None of the selected repositories can be fetched')} onclick={handlers.fetch}>
    <Icon name="refresh" /><span class="t">Fetch</span>{#if targets.fetchable.length}<small>{targets.fetchable.length}</small>{/if}
  </button>
  <button disabled={busy || !targets.behind.length} title={targets.behind.length ? `Pull ${targets.behind.length} selected` : reason('No selected repository is behind its upstream')} onclick={handlers.pull}>
    <Icon name="download" /><span class="t">Pull</span>{#if targets.behind.length}<small>{targets.behind.length}</small>{/if}
  </button>
  <button disabled={busy || !targets.pushable.length} title={targets.pushable.length ? `Push ${targets.pushable.length} selected` : reason('Nothing to push in the selected repositories')} onclick={handlers.push}>
    <Icon name="upload" /><span class="t">Push</span>{#if targets.pushable.length}<small>{targets.pushable.length}</small>{/if}
  </button>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="bulk-more-wrap" onkeydown={onKey}>
    <button bind:this={more} class="more" aria-haspopup="menu" aria-expanded={open} title="More actions for the selection" onclick={() => (open = !open)}>
      <Icon name="more" /><span class="t">More</span>
    </button>
    {#if open}
      <div class="bulk-more" role="menu" bind:this={menu}>
        <button role="menuitem" disabled={busy || !targets.offRef.length} title={targets.offRef.length ? undefined : reason('Every selected repository is already on its branch')} onclick={() => run(handlers.switch)}>
          <Icon name="branch" />Switch to the set’s branch{#if targets.offRef.length}<small>{targets.offRef.length}</small>{/if}
        </button>
        <button role="menuitem" disabled={busy || !refEligible} title={busy || !refEligible ? reason(refReason) : undefined} onclick={() => run(() => handlers.ref(more))}><Icon name="tag" />Choose branch, tag or commit…</button>
        <button role="menuitem" disabled={checking} onclick={() => run(handlers.check)}>
          {#if checking}<span class="spin"></span>{:else}<Icon name="search" />{/if}Check which refs exist
        </button>
        <button role="menuitem" disabled={busy || !targets.cloned.length} title={targets.cloned.length ? undefined : reason('Clone the selected repositories first')} onclick={() => run(handlers.branch)}><Icon name="plus" />New branch…</button>
        <button role="menuitem" disabled={busy || !targets.cloned.length} title={targets.cloned.length ? undefined : reason('Clone the selected repositories first')} onclick={() => run(handlers.cleanup)}><Icon name="trash" />Clean up merged branches…</button>
        <button role="menuitem" disabled={busy || !dirty} title={dirty ? undefined : reason('No selected repository has changes')} onclick={() => run(handlers.commit)}>
          <Icon name="check" />Commit changes…{#if dirty > 1}<small>{dirty}</small>{/if}
        </button>
      </div>
    {/if}
  </div>
  <button class="x" aria-label="Clear selection" title="Clear selection" onclick={handlers.clear}><Icon name="close" /></button>
</div>
