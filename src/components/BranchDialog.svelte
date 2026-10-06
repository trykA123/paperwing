<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type SetItem } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import { dialogOut } from '../lib/motion';
  import Icon from './Icon.svelte';
  import RefSelect, { type RefGroup } from './RefSelect.svelte';

  let { request }: { request: NonNullable<typeof app.gitDialog> } = $props();
  let dialog: HTMLDialogElement;
  let input: HTMLInputElement;
  let name = $state(''), start = $state(''), switchTo = $state(true);
  let busy = $state(false), error = $state('');
  const HEAD_GROUP: RefGroup = { label: 'Current', icon: 'commit', tone: 'commit', names: ['HEAD'] };
  let groups = $state<RefGroup[]>([HEAD_GROUP]);
  let results = $state<Record<string, string>>({});
  let separate = $state(false), folderInput = $state('');
  let copyItem = $state<SetItem | null>(null);

  const targets = $derived(request.targets ?? [{ path: request.path, name: request.name }]);
  const many = $derived(targets.length > 1);
  const canCopy = $derived(!many && !!request.itemId);
  const local = $derived(app.local[request.path]);
  const dirtyRepos = $derived(targets.filter(target => app.local[target.path]?.dirty).length);
  const clean = $derived(name.trim());
  const defaultFolder = $derived(clean.replace(/[\\/:*?"<>|\x00-\x1f\s]+/g, '-').replace(/^[.\-\s]+|[.\-\s]+$/g, ''));
  const folder = $derived((folderInput.trim() || defaultFolder).replace(/[\\/:*?"<>|\x00-\x1f]/g, '').replace(/^[.\s]+|[.\s]+$/g, ''));
  const folderProblem = $derived(!separate ? ''
    : !folder ? 'Give the new folder a name.'
    : app.set.items.some(item => item.id !== copyItem?.id && app.folderOf(item).toLowerCase() === folder.toLowerCase()) ? `A row named ${folder} is already in this set.`
    : '');
  const problem = $derived(
    !clean ? ''
    : /[\s~^:?*[\\]/.test(clean) || clean.includes('..') || clean.includes('@{') || clean.startsWith('-') || clean.startsWith('/') || clean.endsWith('/')
      || clean.endsWith('.') || clean.endsWith('.lock') || clean.includes('//') || clean === '@' ? 'Branch names cannot contain spaces, ~ ^ : ? * [ \\ .. or @{, and cannot start with - or end with . or .lock.'
    : '');

  async function createInFolder() {
    const source = app.set.items.find(item => item.id === request.itemId);
    if (!source) { error = 'This repository is no longer in the set.'; return; }
    busy = true; error = '';
    try {
      if (!copyItem) {
        if (app.running || app.gitBusy) throw new Error('Wait for the running Git operation to finish first.');
        const item = app.addCopy(source, folder);
        copyItem = item;
        await app.checkExists([app.dest(item)]);
        if (app.exists[app.dest(item)]) {
          await app.removeItem(item.id);
          copyItem = null;
          throw new Error(`A folder named ${folder} already exists on disk.`);
        }
        if (!await app.cloneAndWait([item])) {
          await app.removeItem(item.id);
          copyItem = null;
          throw new Error('The copy could not be cloned. The Activity log has the details.');
        }
      }
      const added = copyItem;
      await api.createBranch(app.dest(added), clean, start.trim() || null, true);
      app.setRef(added, { type: 'branch', name: clean });
      await app.checkExists([app.dest(added)]);
      app.toast(`Created ${clean} in its own folder, ${folder}. Push it to publish the branch.`, 'success');
      app.gitDialog = null;
    } catch (reason) {
      error = reason instanceof Error ? reason.message : String(reason);
      busy = false;
    }
  }

  async function create() {
    if (!clean || problem || busy) return;
    if (separate && canCopy) { if (!folderProblem) await createInFolder(); return; }
    if (switchTo && !await app.guardBuffers()) return;
    busy = true; error = '';
    const todo = targets.filter(target => results[target.path] !== 'ok');
    for (const target of todo) {
      try {
        await api.createBranch(target.path, clean, start.trim() || null, switchTo);
        results[target.path] = 'ok';
      } catch (reason) { results[target.path] = String(reason); }
    }
    await app.checkExists(targets.map(target => target.path));
    const failed = targets.filter(target => results[target.path] !== 'ok');
    if (!failed.length) {
      app.toast(many ? `Created ${clean} in ${targets.length} repositories` : switchTo ? `Created and switched to ${clean}` : `Created branch ${clean}`, 'success');
      app.gitDialog = null;
      return;
    }
    if (!many) error = results[failed[0].path];
    else app.toast(`Created ${clean} in ${targets.length - failed.length} of ${targets.length} repositories`, 'warn');
    busy = false;
  }

  function close() {
    if (!busy) app.gitDialog = null;
  }

  onMount(() => {
    dialog.showModal();
    input.focus();
    Promise.all(targets.map(target => api.repositoryTree(target.path))).then(trees => {
      const common = (pick: (tree: (typeof trees)[number]) => string[]) => {
        const sets = trees.map(tree => new Set(pick(tree)));
        return [...sets[0]].filter(item => sets.every(set => set.has(item))).sort();
      };
      groups = [
        HEAD_GROUP,
        { label: 'Branches', icon: 'branch', tone: 'branch', names: common(tree => tree.branches.map(item => item.name)), labels: Object.fromEntries(trees.flatMap(tree => tree.branches.map(item => [item.name, item.label ?? item.name]))) },
        { label: 'Tags', icon: 'tag', tone: 'tag', names: common(tree => tree.tags.map(item => item.name)), labels: Object.fromEntries(trees.flatMap(tree => tree.tags.map(item => [item.name, item.label ?? item.name]))) },
        { label: 'Remote branches', icon: 'branch', tone: 'repo', names: common(tree => tree.remotes.flatMap(remote => remote.refs.map(item => item.name)).filter(item => !item.endsWith('/HEAD'))), labels: Object.fromEntries(trees.flatMap(tree => tree.remotes.flatMap(remote => remote.refs.map(item => [item.name, item.label ?? item.name])))) },
      ].filter(group => group.names.length) as RefGroup[];
    }).catch(() => {});
  });
</script>

<dialog class="operation-dialog branch-dialog" bind:this={dialog} out:dialogOut|global aria-label="New branch"
  oncancel={event => { event.preventDefault(); close(); }}>
  <header>
    <h2>New branch</h2>
    <span class="mut">{many ? `${targets.length} repositories` : request.name}</span>
    <span class="grow"></span>
    <button class="icon" title="Close" aria-label="Close" disabled={busy} onclick={close}><Icon name="close" /></button>
  </header>

  <form class="branch-form" onsubmit={event => { event.preventDefault(); void create(); }}>
    {#if many}
      <ul class="branch-targets" aria-label="Repositories">
        {#each targets as target (target.path)}
          {@const result = results[target.path]}
          <li><Icon name="folder" size={12} tone="repo" /><span class="grow">{target.name}</span>
            {#if result === 'ok'}<span class="okc">created</span>{:else if result}<span class="err" title={result}>{result}</span>{/if}</li>
        {/each}
      </ul>
    {/if}

    <label class="fld"><span>Branch name</span>
      <input bind:this={input} bind:value={name} placeholder="feature/my-change" spellcheck="false" autocomplete="off" disabled={busy} />
    </label>
    {#if problem}<div class="banner warn">{problem}</div>{/if}

    <div class="fld"><label for="branch-start"><span>Start from <em class="mut">{many ? 'must exist in every repository; defaults to each current commit' : `defaults to the current commit${local?.branch ? ` (${local.branchLabel ?? local.branch})` : ''}`}</em></span></label>
      <RefSelect id="branch-start" bind:value={start} {groups} placeholder="HEAD" disabled={busy} />
    </div>

    {#if canCopy}
      <label class="check"><input type="checkbox" bind:checked={separate} disabled={busy || !!copyItem} /> Keep it in its own folder, next to {request.name}</label>
      {#if separate}
        <div class="fld"><label for="branch-folder"><span>Folder name</span></label>
          <input id="branch-folder" bind:value={folderInput} placeholder={defaultFolder || 'folder-name'} spellcheck="false" autocomplete="off" disabled={busy} />
          <small class="hint">{request.name} stays on its current branch. A new copy is cloned into <code>{folder || '…'}</code>, switched to {clean || 'the new branch'}, and added to this set as its own row.</small>
        </div>
        {#if folderProblem}<div class="banner warn">{folderProblem}</div>{/if}
        {#if busy}<p class="hint"><span class="spin"></span> {app.running ? 'Cloning the copy…' : 'Working…'}</p>{/if}
      {/if}
    {/if}

    {#if !separate}<label class="check"><input type="checkbox" bind:checked={switchTo} disabled={busy} /> Switch to the new branch</label>{/if}
    {#if !separate && switchTo && dirtyRepos}<div class="banner info">Uncommitted changes in {dirtyRepos} {dirtyRepos === 1 ? 'repository' : 'repositories'} will come with you to the new branch.</div>{/if}
    {#if error}<div class="banner err" role="alert">{error}</div>{/if}

    <footer>
      <span class="hint">The branch is created locally; nothing is pushed.</span>
      <span class="grow"></span>
      <button type="button" class="btn" disabled={busy} onclick={close}>Cancel</button>
      <button type="submit" class="btn dark" disabled={!clean || !!problem || !!folderProblem || busy}>{#if busy}<span class="spin"></span>{:else}<Icon name="branch" />{/if} {many ? `Create in ${targets.length - Object.values(results).filter(value => value === 'ok').length} repositories` : separate ? 'Create in new folder' : 'Create branch'}</button>
    </footer>
  </form>
</dialog>
