<script lang="ts">
  import { untrack } from 'svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { api } from '../../lib/api';
  import { describeError } from '../../lib/errors';
  import { app } from '../../lib/state.svelte';
  import { createGithubReleases, type CreateRow, type ReleaseRow } from '../../lib/tags-set';

  let { rows, tag, message, onbusy }: { rows: CreateRow[]; tag: string; message: string; onbusy: (busy: boolean) => void } = $props();
  let notes = $state(untrack(() => message));
  let draft = $state(true);
  let running = $state(false);
  let results = $state<ReleaseRow[]>([]);

  async function create() {
    if (running || results.length) return;
    running = true;
    onbusy(true);
    try { results = await createGithubReleases(rows, { tag, notes, draft }, api, row => { results = [...results, row]; }); }
    finally { running = false; onbusy(false); }
  }

  async function openRelease(event: MouseEvent, url: string) {
    event.preventDefault();
    try { await openUrl(url); }
    catch (reason) { app.toast(describeError(reason, 'open the GitHub release'), 'error'); }
  }
</script>

<section class="tag-section" aria-label="GitHub release">
  <h3>Create GitHub release</h3>
  {#if !results.length && !running}
    <label class="fld"><span>Release notes</span><textarea bind:value={notes} rows="2" spellcheck="false"></textarea></label>
    <label class="check"><input type="checkbox" bind:checked={draft} /> Save as draft</label>
    <button type="button" class="btn" onclick={() => void create()}>Create GitHub release</button>
  {/if}
  {#if running}<p class="tag-note" role="status">Creating releases…</p>{/if}
  {#if results.length}
    <ul class="tag-targets" aria-label="Release results" aria-live="polite">
      {#each results as row (row.path)}
        <li><span class="grow tag-name">{row.name}</span>
          {#if row.release}<a href={row.release.url} target="_blank" rel="noopener noreferrer" onclick={event => { if (row.release) void openRelease(event, row.release.url); }}>{row.release.draft ? 'Open draft release' : 'Open release'}</a>
          {:else}<span class="err">Release failed</span><p class="tag-error err" role="alert">{row.error}</p>{/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>
