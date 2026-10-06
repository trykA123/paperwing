<script lang="ts">
  import type { Progress, RefKind } from '../../lib/api';

  let { j, st, stale = false, refErr, phaseLabel, jobRunning, clash, destination, refKind, onopen }: {
    j: Progress | undefined; st: string; stale?: boolean; refErr: string | undefined; phaseLabel: string; jobRunning: boolean;
    clash: boolean; destination: string; refKind: RefKind; onopen: () => void;
  } = $props();
</script>

<div>
  {#if j}
    <div class="status">
      <div class="ph ph-{j.phase}" title={j.msg}><b>{phaseLabel}</b>{j.msg}</div>
      {#if jobRunning}
        <div class="bar"><i style:width="{j.pct}%"></i></div>
      {:else if j.phase === 'done' || j.phase === 'skipped'}
        <button class="link" style="text-align:left" onclick={onopen}>Open in VS Code</button>
      {/if}
    </div>
  {:else if st === 'missing'}
    <div class="ph ph-warn"><b>!</b>{refKind === 'tag' ? 'Tag' : refKind === 'branch' ? 'Branch' : 'Commit'} not found</div>
  {:else if clash}
    <div class="ph ph-failed" title={destination}><b>!</b>Same folder as another row</div>
  {:else if refErr}
    <div class="ph ph-warn" title={refErr}><b>!</b>{refErr}</div>
  {:else if st === 'unverified'}
    <div class="ph ph-ready">Commit is checked while cloning</div>
  {:else}
    <div class="ph ph-ready">{st === 'ok' ? (stale ? 'Ready · ref verified earlier' : 'Ready · ref verified') : 'Ready'}</div>
  {/if}
</div>
