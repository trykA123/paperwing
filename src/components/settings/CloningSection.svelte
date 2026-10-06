<script lang="ts">
  import type { OnExisting } from '../../lib/api';

  let { shallow = $bindable(), onExisting = $bindable(), parallel, running, onparallel }: {
    shallow: boolean; onExisting: OnExisting; parallel: number; running: boolean; onparallel: (event: Event) => void;
  } = $props();
</script>

<section class="settings-section">
    <div class="section-head"><div class="grow"><h2>Cloning</h2><p class="mut">Defaults used whenever you clone, fetch or switch a set.</p></div></div>
    <div class="card setting-list">
      <label class="setting-row">
        <span class="setting-label"><b>Shallow clone</b><small>Download only the latest commit; older history is not fetched.</small></span>
        <input type="checkbox" role="switch" bind:checked={shallow} disabled={running} />
      </label>
      <label class="setting-row">
        <span class="setting-label"><b>Parallel clones</b><small>How many repositories are cloned at the same time (1–8).</small></span>
        <input class="parallel-input" type="number" min="1" max="8" step="1" value={parallel} onchange={onparallel} disabled={running} />
      </label>
      <label class="setting-row">
        <span class="setting-label"><b>Existing folders</b><small>What to do when the destination folder already exists.</small></span>
        <select bind:value={onExisting} disabled={running}>
          <option value="fetch">Fetch &amp; checkout</option>
          <option value="skip">Skip</option>
          <option value="reclone">Re-clone (keep backup)</option>
        </select>
      </label>
    </div>
    {#if running}<p class="hint">Locked while a Git operation is running.</p>{/if}
  </section>
