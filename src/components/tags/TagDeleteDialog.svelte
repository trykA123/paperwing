<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { api, type TagInfo } from '../../lib/api';
  import { dialogOut } from '../../lib/motion';
  import { plural } from '../../lib/plural';
  import { tagFlow } from '../../lib/tag-flow.svelte';
  import { deleteLocalTags, planTags, shortId, type PlanRow, type RemoveRow, type TagTarget } from '../../lib/tags-set';
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';

  let { targets, tag: initial }: { targets: TagTarget[]; tag: string } = $props();
  let dialog: HTMLDialogElement;
  let input = $state<HTMLInputElement>();
  let name = $state(untrack(() => initial));
  let known = $state<string[]>([]);
  let plan = $state<PlanRow[]>([]);
  let busy = $state(false);
  let local = $state<Record<string, RemoveRow>>({});
  let sequence = 0;
  let lists: Record<string, Promise<TagInfo[]>> = {};
  let listsAt = -1;

  const many = $derived(targets.length > 1);
  const tag = $derived(name.trim());
  const present = $derived(plan.filter(row => row.existing && local[row.path]?.status !== 'removed'));
  const removed = $derived(Object.values(local).filter(row => row.status === 'removed').length);

  async function refresh() {
    const mine = ++sequence;
    if (listsAt !== tagFlow.revision) { lists = {}; listsAt = tagFlow.revision; }
    const cached = { listTags: (path: string) => (lists[path] ??= api.listTags(path)), localStatus: async () => [] };
    const next = tag ? await planTags(targets, tag, cached) : targets.map(target => ({ ...target, existing: null, error: null }));
    if (mine === sequence) plan = next;
  }
  $effect(() => { void tag; void tagFlow.revision; void refresh(); });

  async function listKnown() {
    const lists = await Promise.all(targets.map(target => api.listTags(target.path).catch(() => [])));
    known = [...new Set(lists.flat().map(entry => entry.name))].sort();
  }

  async function deleteLocal() {
    if (busy || !tag || !present.length) return;
    busy = true;
    const rows = present;
    const guarded = await tagFlow.guarded(rows.map(row => row.path), () => deleteLocalTags(rows, tag, api, (_, row) => { local[row.path] = row; }));
    busy = false;
    if (!guarded.ran) { app.toast('A Git operation is already running', 'warn'); return; }
    const failures = guarded.value.filter(row => row.status === 'failed').length;
    app.toast(failures ? `Deleted ${tag} in ${guarded.value.length - failures} of ${plural(guarded.value.length, 'repository', 'repositories')}` : `Deleted ${tag} locally in ${plural(guarded.value.length, 'repository', 'repositories')}`, failures ? 'warn' : 'success');
  }

  const close = () => { if (!busy) tagFlow.close(); };
  onMount(() => {
    dialog.showModal();
    input?.focus();
    void listKnown();
    return () => tagFlow.restoreFocus();
  });
</script>

<dialog class="operation-dialog tag-dialog" bind:this={dialog} out:dialogOut|global aria-label="Delete tag" oncancel={event => { event.preventDefault(); close(); }}>
  <header>
    <h2>Delete tag</h2>
    <span class="mut">{many ? plural(targets.length, 'repository', 'repositories') : targets[0].name}</span>
    <span class="grow"></span>
    <button class="icon" title="Close" aria-label="Close" disabled={busy} onclick={close}><Icon name="close" /></button>
  </header>

  <div class="tag-form">
    <label class="fld"><span>Tag name</span>
      <input bind:this={input} bind:value={name} list="tag-known" class="mono" placeholder="v2.4.0" spellcheck="false" autocomplete="off" disabled={busy} />
      <datalist id="tag-known">{#each known as entry (entry)}<option value={entry}></option>{/each}</datalist>
    </label>
    <p class="tag-note mut">Deleting removes only your local copy. Skein does not delete tags on a remote.</p>

    <ul class="tag-targets" aria-label="Repositories">
      {#each plan as row (row.path)}
        <li>
          <Icon name="folder" size={12} tone="repo" />
          <span class="grow tag-name">{row.name}</span>
          {#if local[row.path]}
            <span class={local[row.path].status === 'removed' ? 'okc' : 'err'}>{local[row.path].status === 'removed' ? 'Deleted locally' : 'Not deleted'}</span>
          {:else if !tag}<span class="mut">Enter a tag name</span>
          {:else if row.error}<span class="err">Could not read tags</span>
          {:else if row.existing}<span class="mono mut" title="Commit the tag points at">{shortId(row.existing.commit)}</span>
          {:else}<span class="mut">No local tag</span>{/if}
          {#each [local[row.path]?.error, row.error] as message}{#if message}<p class="tag-error err" role="alert">{message}</p>{/if}{/each}
        </li>
      {/each}
    </ul>

    <footer>
      <span class="hint">{removed ? `${removed} deleted locally` : 'Nothing is deleted until you choose the button.'}</span>
      <button type="button" class="btn" disabled={busy} onclick={close}>{removed ? 'Done' : 'Cancel'}</button>
      <button type="button" class="btn danger" disabled={busy || !present.length} onclick={deleteLocal}>{#if busy}<span class="spin"></span>{:else}<Icon name="trash" />{/if} Delete {present.length > 1 ? `${present.length} local tags` : 'local tag'}</button>
    </footer>
  </div>
</dialog>
