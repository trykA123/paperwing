<script lang="ts">
  import type { LocalStatus } from '../../lib/api';
  import Icon from '../Icon.svelte';

  let { l, on, checkoutRef, running, gitBusy, actions }: {
    l: LocalStatus | undefined; on: boolean; checkoutRef: string; running: boolean; gitBusy: boolean;
    actions: { commit: () => void; switch: () => void; pull: () => void; push: () => void };
  } = $props();
</script>

<div>
  {#if !l || !l.exists}
    <div class="ph ph-ready">Not cloned yet</div>
  {:else if !l.repo}
    <div class="ph ph-warn"><b>!</b>Folder is not a git repo</div>
  {:else if l.error}
    <div class="ph ph-warn" title={l.error}><b>!</b>{l.error}</div>
  {:else}
    {@const kind = l.branch ? 'branch' : l.tag ? 'tag' : 'commit'}
    <div class="local">
      <div class="lhead">
        <span class="t-{kind}"><Icon name={kind} /></span>
        <span class="lref" class:match={on} title={l.upstream ? `Tracks ${l.upstreamLabel ?? l.upstream}` : 'No upstream branch'}>{l.branchLabel ?? l.branch ?? l.tagLabel ?? l.tag ?? l.sha}</span>
        {#if on}<span class="lok" title="On the checkout ref"><Icon name="check" size={12} /></span>{/if}
        {#if l.ahead}<span class="ab up" title="{l.ahead} local commit(s) not pushed">↑{l.ahead}</span>{/if}
        {#if l.behind}<span class="ab down" title="{l.behind} commit(s) to pull, as of the last fetch">↓{l.behind}</span>{/if}
        {#if l.dirty}<span class="ab dirty" title="{l.dirty} changed or untracked file(s)">{l.dirty} changed</span>{/if}
      </div>
      {#if l.dirty || !on || (l.branch && (l.behind || l.ahead || !l.upstream))}
        <div class="lacts">
          {#if l.dirty}
            <button class="mini record" disabled={running} title="Review, stage and commit the {l.dirty} changed file(s)" onclick={actions.commit}>Commit…</button>
          {/if}
          {#if !on}
            <button class="mini branch" disabled={running} title="Fetch and check out {checkoutRef}" onclick={actions.switch}>Switch to {checkoutRef}</button>
          {/if}
          {#if l.branch && l.behind}
            <button class="mini sync" disabled={running} title="Fast-forward {l.branchLabel ?? l.branch} to {l.upstreamLabel ?? l.upstream} ({l.behind} commit(s))" onclick={actions.pull}>Pull</button>
          {/if}
          {#if l.branch && (l.ahead || !l.upstream)}
            <button class="mini sync" disabled={gitBusy} title={l.upstream ? `Push ${l.ahead} commit(s) to ${l.upstreamLabel ?? l.upstream}` : `Push ${l.branchLabel ?? l.branch} to the remote and track it`}
              onclick={actions.push}>{l.upstream ? 'Push' : 'Publish'}</button>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>
