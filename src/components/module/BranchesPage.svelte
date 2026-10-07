<script lang="ts">
  import type { SetItem } from '../../lib/api';
  import { BRANCH_SECTIONS, branches, checkedOut } from '../../lib/branches.svelte';
  import { describeRow } from '../../lib/formation-row';
  import { plural } from '../../lib/plural';
  import { stashFlow } from '../../lib/stash-flow.svelte';
  import { app } from '../../lib/state.svelte';
  import { tagFlow } from '../../lib/tag-flow.svelte';
  import EmptyState from '../EmptyState.svelte';
  import Icon from '../Icon.svelte';
  import SyncRails from '../set/SyncRails.svelte';
  import ModuleTable from './ModuleTable.svelte';
  import PageFrame from './PageFrame.svelte';

  const section = $derived(BRANCH_SECTIONS.find(entry => entry.id === branches.section)!);
  const busy = $derived(app.running || app.gitBusy || app.clonePreparing);
  const act = (run: (items: SetItem[], opener: Element) => void, item: SetItem, event: MouseEvent) => run([item], event.currentTarget as Element);
</script>

<PageFrame crumb="Branches & tags" title={section.title} sub="{plural(branches.rows.length, 'cloned repository', 'cloned repositories')} in {app.set.name} · {section.hint}">
  <ModuleTable label="{section.label} in {app.set.name}" columns={['Repository', 'Checked out', 'Sync', 'Actions']} cols="minmax(180px, 1.4fr) minmax(140px, 1fr) minmax(150px, 1.1fr) minmax(170px, 250px)"
    items={branches.rows} key={item => item.id} bind:page={branches.page}>
    {#snippet cells(item: SetItem, index: number)}
      {@const model = describeRow(item, { focused: false, canAct: !busy })}
      <div class="fm-row" role="row" aria-rowindex={index + 2}>
        <div class="fm-cell fm-repo" role="gridcell">
          <span class="fm-name static">{model.folder}</span>
          <small class="fm-sub"><span class="fm-org" title={model.sub}>{model.sub}</span></small>
        </div>
        <div class="fm-cell fm-branch" role="gridcell">
          <span class="fm-ref" title="Checked out in this folder"><span class="t-branch"><Icon name={model.local?.branch ? 'branch' : 'tag'} /></span><span class="nm">{checkedOut(model.local) || 'detached'}</span></span>
        </div>
        <div class="fm-cell fm-syncc" role="gridcell"><SyncRails view={model.sync} /></div>
        <div class="fm-cell fm-next module-actions" role="gridcell">
          {#if branches.section === 'cleanup'}
            <button class="btn small fm-action" disabled={busy} title="Delete the merged branches of this repository" onclick={() => app.openCleanupDialog([item])}><Icon name="trash" tone="branch" />Clean up…</button>
          {:else if branches.section === 'tags'}
            <button class="btn small fm-action" disabled={busy} onclick={event => act((items, opener) => tagFlow.openCreate(items, opener), item, event)}><Icon name="tag" tone="tag" />Tag…</button>
            <button class="btn small fm-action" disabled={busy} title="Delete a tag here and on the remote" onclick={event => act((items, opener) => tagFlow.openDelete(items, opener), item, event)}><Icon name="trash" tone="danger" />Delete…</button>
          {:else}
            <button class="btn small fm-action" disabled={busy} title="Stash the {model.local?.dirty ?? 0} uncommitted files" onclick={event => act((items, opener) => stashFlow.openPush(items, opener), item, event)}><Icon name="stash" tone="record" />Stash…</button>
            {#if branches.canSwitch(item)}<button class="btn small fm-action" disabled={busy} title="Switch to {item.ref.name}, stashing changes first" onclick={event => act((items, opener) => stashFlow.openSwitch(items, opener), item, event)}><Icon name="branch" tone="branch" />Switch…</button>{/if}
          {/if}
        </div>
      </div>
    {/snippet}
    {#snippet empty()}
      {#if branches.section === 'stash' && branches.cloned.length}
        <EmptyState icon="check" title="No uncommitted changes" hint="Every cloned repository of {app.set.name} has a clean working tree." />
      {:else}
        <EmptyState icon="folder" title="No cloned repositories" hint="Clone the repositories of {app.set.name} first." />
      {/if}
    {/snippet}
  </ModuleTable>
</PageFrame>
