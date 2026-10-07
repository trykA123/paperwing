<script lang="ts">
  import type { Ref, SetItem } from '../../lib/api';
  import { bulkTargets } from '../../lib/formation';
  import { applyRef } from '../../lib/ref-apply';
  import { pullFlow, pullable } from '../../lib/pull-flow.svelte';
  import { isRemoteItem } from '../../lib/repositories';
  import { needsClone, pushTarget, rowFacts } from '../../lib/row-actions';
  import { stashFlow, switchable } from '../../lib/stash-flow.svelte';
  import { app } from '../../lib/state.svelte';
  import { tagFlow } from '../../lib/tag-flow.svelte';
  import RefPicker from '../RefPicker.svelte';
  import BulkBar from '../set/BulkBar.svelte';
  import AddToSetMenu from './AddToSetMenu.svelte';

  let { selected, extend, busy }: { selected: SetItem[]; extend: { count: number; run: () => void } | null; busy: boolean } = $props();

  let checking = $state(false);
  let picker = $state<{ items: SetItem[]; anchor: DOMRect } | null>(null);
  let addAnchor = $state<HTMLElement | null>(null);
  const targets = $derived(bulkTargets(selected, rowFacts));
  const dirty = $derived(targets.cloned.filter(item => (app.local[app.dest(item)]?.dirty ?? 0) > 0));
  const remote = $derived(selected.filter(item => !item.path && !isRemoteItem(item)));
  const cloneable = $derived(selected.filter(item => isRemoteItem(item) || needsClone(item)));
  const moreButton = () => document.querySelector<HTMLElement>('.fm-bar .more');

  export function pickRef(items: SetItem[], anchor: DOMRect) { picker = { items, anchor }; }

  function pick(ref: Ref) {
    const chosen = picker!.items;
    picker = null;
    applyRef(chosen, ref);
  }

  async function checkRefs() {
    checking = true;
    await app.ensureRefs(selected.filter(item => !item.path).map(item => item.url), true);
    checking = false;
  }

  const handlers = {
    fetch: () => app.startClone(targets.fetchable, 'fetch'),
    pull: () => app.startClone(targets.behind, 'pull'),
    push: () => app.pushRepos(targets.pushable.map(pushTarget)),
    switch: () => app.startClone(targets.offRef, 'switch'),
    ref: (anchor: HTMLElement) => { if (remote.length) pickRef(remote, anchor.getBoundingClientRect()); },
    clone: () => app.repositories.cloneItems(cloneable),
    check: checkRefs,
    branch: () => app.openBranchDialog(targets.cloned),
    cleanup: () => app.openCleanupDialog(targets.cloned),
    commit: () => { if (dirty[0]) app.openGitDialog('commit', dirty[0]); },
    stash: () => stashFlow.openPush(dirty, moreButton()),
    switchStash: () => stashFlow.openSwitch(selected, moreButton()),
    tag: () => tagFlow.openCreate(targets.cloned, moreButton()),
    deleteTag: () => tagFlow.openDelete(targets.cloned, moreButton()),
    pulls: () => pullFlow.openBulk(selected, moreButton()),
    addToSet: () => { addAnchor = moreButton() ?? document.body; },
    clear: () => { for (const item of selected) item.on = false; },
  };
</script>

<BulkBar count={selected.length} refEligible={remote.length} {targets} dirty={dirty.length} stashSwitch={switchable(selected).length} pullable={pullable(selected).length} cloneable={cloneable.length} {busy} {checking} {extend} {handlers} />

{#if picker}<RefPicker items={picker.items} anchor={picker.anchor} onclose={() => (picker = null)} onpick={pick} />{/if}
{#if addAnchor}<AddToSetMenu items={selected} anchor={addAnchor} onclose={() => (addAnchor = null)} />{/if}
