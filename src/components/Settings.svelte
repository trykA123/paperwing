<script lang="ts">
  import { confirm } from '../lib/confirm';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { app, uid } from '../lib/state.svelte';
  import { api, type Source, type SourceKind, type Theme } from '../lib/api';
  import { CODE_FONTS, UI_FONTS } from '../lib/appearance';
  import Icon, { type IconName, type IconTone } from './Icon.svelte';

  type Section = 'sources' | 'appearance' | 'cloning';
  const SECTIONS: { id: Section; label: string; icon: IconName; tone?: IconTone }[] = [
    { id: 'sources', label: 'Sources', icon: 'folder', tone: 'folder' },
    { id: 'appearance', label: 'Appearance', icon: 'theme', tone: 'brand' },
    { id: 'cloning', label: 'Cloning', icon: 'download', tone: 'branch' },
  ];
  let section = $state<Section>('sources');

  function setParallel(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const count = input.valueAsNumber;
    if (Number.isFinite(count)) app.ws.parallel = Math.max(1, Math.min(8, Math.round(count)));
    input.value = String(app.ws.parallel);
  }

  const THEMES: { id: Theme; label: string }[] = [
    { id: 'system', label: 'System' },
    { id: 'light', label: 'Light' },
    { id: 'dark', label: 'Dark' },
  ];

  const KINDS: { id: SourceKind; title: string; desc: string }[] = [
    { id: 'ghe', title: 'GitHub Enterprise', desc: 'Your self-hosted GitHub server' },
    { id: 'github', title: 'GitHub.com', desc: 'Organizations on github.com' },
    { id: 'manual', title: 'Manual URLs', desc: 'Any host without an API (Bitbucket…)' },
  ];

  let draft = $state<Source | null>(null);
  let isNew = $state(false);
  let token = $state('');
  let orgInput = $state('');
  let urlsText = $state('');
  let myOrgs = $state<string[]>([]);
  let busy = $state<'' | 'token' | 'orgs'>('');
  let msg = $state<{ ok: boolean; text: string; where: 'token' | 'orgs' | 'form' } | null>(null);
  let tokenSaved = $state<Record<string, boolean>>({});

  $effect(() => {
    for (const s of app.sources) api.hasToken(s.id).then(v => (tokenSaved[s.id] = v));
  });

  const tokenUrl = $derived(
    draft && draft.kind !== 'manual' && draft.host
      ? `https://${draft.host}/settings/tokens/new?scopes=repo,read:org&description=PaperWing`
      : '',
  );

  function reset() {
    token = ''; orgInput = ''; myOrgs = []; msg = null; busy = '';
  }

  function startNew() {
    draft = { id: uid(), name: 'GitHub.com', kind: 'github', host: 'github.com', orgs: [], urls: [] };
    urlsText = '';
    isNew = true;
    reset();
  }

  function edit(s: Source) {
    draft = structuredClone($state.snapshot(s)) as Source;
    urlsText = draft.urls.join('\n');
    isNew = false;
    reset();
  }

  function setKind(k: SourceKind) {
    if (!draft) return;
    const defaultName = KINDS.some(x => x.title === draft!.name);
    draft.kind = k;
    if (k === 'github') draft.host = 'github.com';
    else if (draft.host === 'github.com') draft.host = '';
    if (defaultName) draft.name = KINDS.find(x => x.id === k)!.title;
  }

  function addOrg(name = orgInput) {
    const o = name.trim();
    if (draft && o && !draft.orgs.some(x => x.toLowerCase() === o.toLowerCase())) draft.orgs.push(o);
    orgInput = '';
  }

  function snapshot(): Source {
    const s = $state.snapshot(draft!) as Source;
    s.host = s.host.trim().replace(/^https?:\/\//, '').replace(/\/.*$/, '');
    s.urls = s.kind === 'manual' ? urlsText.split(/\r?\n/).map(x => x.trim()).filter(Boolean) : [];
    if (s.kind === 'manual') s.orgs = [];
    return s;
  }

  async function saveToken() {
    if (!draft || !token.trim()) return;
    await api.setToken(draft.id, token.trim());
    tokenSaved[draft.id] = true;
    token = '';
  }

  async function run(where: 'token' | 'orgs', task: () => Promise<void>) {
    msg = null;
    const missing = !draft?.host.trim() ? 'Enter the host first.'
      : !token.trim() && !tokenSaved[draft!.id] ? 'Paste a personal access token first.'
      : '';
    if (missing) { msg = { ok: false, text: missing, where }; return; }
    busy = where;
    try { await saveToken(); await task(); } catch (e) { msg = { ok: false, text: String(e), where }; }
    busy = '';
  }

  const test = () => run('token', async () => {
    msg = { ok: true, text: `Connected as ${await api.testSource(snapshot())}. The token works.`, where: 'token' };
  });
  const loadOrgs = () => run('orgs', async () => {
    myOrgs = await api.listUserOrgs(snapshot());
    msg = myOrgs.length
      ? { ok: true, text: `Found ${myOrgs.length}. Click the ones you clone from.`, where: 'orgs' }
      : { ok: true, text: 'Your account is not a member of any organization. Type names manually.', where: 'orgs' };
  });

  async function save() {
    const s = snapshot();
    const problem = !s.name.trim() ? 'Give the source a name.'
      : s.kind !== 'manual' && !s.host ? 'Enter the host name.'
      : s.kind !== 'manual' && !s.orgs.length ? 'Add at least one organization (type it and press Enter).'
      : s.kind === 'manual' && !s.urls.length ? 'Paste at least one repository URL.'
      : '';
    if (problem) { msg = { ok: false, text: problem, where: 'form' }; return; }
    try { await saveToken(); } catch (e) { msg = { ok: false, text: String(e), where: 'form' }; return; }
    const i = app.sources.findIndex(x => x.id === s.id);
    if (i >= 0) app.sources[i] = s; else app.sources.push(s);
    draft = null;
    app.toast(`Saved ${s.name}, loading repositories…`, 'success');
    await app.loadRepos(s, true);
  }

  async function cancel() {
    if (isNew && draft) await api.deleteToken(draft.id).catch(() => {});
    draft = null;
  }

  async function remove(s: Source) {
    const ok = await confirm(`Remove "${s.name}"? Its token is deleted from Windows Credential Manager. Sets keep their repos.`,
      { title: 'Remove source', kind: 'warning', okLabel: 'Remove', destructive: true });
    if (!ok) return;
    await api.deleteToken(s.id).catch(() => {});
    app.sources = app.sources.filter(x => x.id !== s.id);
    delete app.repos[s.id];
    delete app.repoErrors[s.id];
  }
</script>

<div class="settings">
  <header class="mh">
    <div class="grow"><div class="crumb">PaperWing</div><h1>Settings</h1></div>
  </header>

  <div class="settings-body">
    <nav class="settings-nav" aria-label="Settings sections">
      {#each SECTIONS as s (s.id)}
        <button class:on={section === s.id} aria-current={section === s.id ? 'page' : undefined} onclick={() => (section = s.id)}>
          <Icon name={s.icon} tone={s.tone} />{s.label}
        </button>
      {/each}
    </nav>

    <div class="settings-content">
  {#if section === 'sources'}
  <section class="settings-section">
    <div class="section-head">
      <div class="grow"><h2>Sources</h2><p class="mut">Where PaperWing finds repositories. Listing uses the host's API; cloning always uses your git + SSH setup.</p></div>
      {#if !draft}<button class="btn dark" onclick={startNew}><Icon name="plus" /> Add source</button>{/if}
    </div>

  {#if !app.sources.length && !draft}
    <div class="banner info">No sources yet. Add GitHub, your GitHub Enterprise server, or manual repository URLs.</div>
  {/if}

  {#if app.sources.length}
    <div class="card">
      {#each app.sources as s (s.id)}
        <div class="srccard">
          <div class="grow">
            <div class="title"><b>{s.name}</b><span class="mut">{s.kind === 'manual' ? `${s.urls.length} URLs` : s.host}</span></div>
            {#if s.orgs.length}<div class="chips">{#each s.orgs as o}<span class="chip">{o}</span>{/each}</div>{/if}
            <small class="mut">
              {(app.repos[s.id] ?? []).length} repositories
              {#if s.kind !== 'manual'} · {#if tokenSaved[s.id]}<span class="okc">token saved</span>{:else}<span class="warn">no token</span>{/if}{/if}
              {#if app.loadingRepos[s.id]} · <span class="spin"></span>{/if}
            </small>
            {#if app.repoErrors[s.id]?.length}<div class="err" style="font-size:var(--fs-sm);margin-top:4px">{app.repoErrors[s.id].join(' · ')}</div>{/if}
          </div>
          {#if s.kind !== 'manual'}
            <button class="btn" disabled={app.loadingRepos[s.id]} onclick={() => app.loadRepos(s, true)}><Icon name="refresh" /> Refresh</button>
          {/if}
          <button class="btn" onclick={() => edit(s)}>Edit</button>
          <button class="btn icon-only" title="Remove" onclick={() => remove(s)}><Icon name="trash" /></button>
        </div>
      {/each}
    </div>
  {/if}

  {#if draft}
    <div class="card form">
      <h2>{isNew ? 'Add source' : `Edit ${draft.name}`}</h2>
      <div class="kinds">
        {#each KINDS as k}
          <button class="kind" class:on={draft.kind === k.id} onclick={() => setKind(k.id)}><b>{k.title}</b><small>{k.desc}</small></button>
        {/each}
      </div>

      <div class="grid2">
        <label class="fld"><span>Display name</span><input bind:value={draft.name} /></label>
        {#if draft.kind === 'ghe'}
          <label class="fld"><span>Host</span><input bind:value={draft.host} placeholder="git.example.com" spellcheck="false" /></label>
        {/if}
      </div>

      {#if draft.kind !== 'manual'}
        <div class="fld">
          <span>Personal access token {#if tokenSaved[draft.id]}<em class="okc">✓ saved in Windows Credential Manager</em>{/if}</span>
          <div class="row">
            <input type="password" bind:value={token} autocomplete="off" spellcheck="false"
              placeholder={tokenSaved[draft.id] ? 'Leave empty to keep the saved token' : 'Paste a token (classic) here'} />
            <button class="btn" disabled={!!busy} onclick={test} title="Asks the server who you are, using this host and token">
              {#if busy === 'token'}<span class="spin"></span>{:else}<Icon name="check" />{/if} Test connection
            </button>
            {#if tokenUrl}<button class="btn" onclick={() => openUrl(tokenUrl)}>Create token ↗</button>{/if}
          </div>
          <small class="hint">Needs the <code>repo</code> and <code>read:org</code> scopes. It is only used to list repositories and commits and is never written to the settings file. <b>Test connection</b> checks that the host and token work.</small>
          {#if msg?.where === 'token'}<div class="banner" class:err={!msg.ok} class:info={msg.ok}>{msg.text}</div>{/if}
        </div>

        <div class="fld">
          <span>Organizations or users
            <button class="btn small" disabled={!!busy} onclick={loadOrgs} title="Lists the organizations your account belongs to">{#if busy === 'orgs'}<span class="spin"></span>{/if}Load my organizations</button>
          </span>
          <div class="chipbox">
            {#each draft.orgs as o (o)}
              <span class="chip">{o}<button title="Remove" onclick={() => (draft!.orgs = draft!.orgs.filter(x => x !== o))}>×</button></span>
            {/each}
            <input bind:value={orgInput} placeholder={draft.orgs.length ? 'Add another…' : 'Type an org name and press Enter'} spellcheck="false"
              onkeydown={e => { if (e.key === 'Enter' || e.key === ',') { e.preventDefault(); addOrg(); } }} onblur={() => addOrg()} />
          </div>
          {#if myOrgs.length}
            <div class="chips">
              <span class="hint">Your organizations:</span>
              {#each myOrgs as o (o)}
                {@const on = draft.orgs.includes(o)}
                <button class="chip pick" class:on onclick={() => (on ? (draft!.orgs = draft!.orgs.filter(x => x !== o)) : addOrg(o))}>{on ? '✓ ' : '+ '}{o}</button>
              {/each}
            </div>
          {/if}
          <small class="hint">Repos from these organizations appear in the sidebar. Use the name from the URL, e.g. <code>example-org</code> in <code>github.com/example-org/demo-project</code>. <b>Load my organizations</b> lists the ones your account belongs to so you can click them.</small>
          {#if msg?.where === 'orgs'}<div class="banner" class:err={!msg.ok} class:info={msg.ok}>{msg.text}</div>{/if}
        </div>
      {:else}
        <label class="fld">
          <span>Repository URLs, one per line</span>
          <textarea rows="8" bind:value={urlsText} spellcheck="false"
            placeholder={'git@github.com:example-org/demo-project.git\nssh://git@git.example.com:7999/projects/demo-project.git'}></textarea>
          <small class="hint">For hosts PaperWing cannot list. The folder/org name comes from the URL path; branches and tags are read with <code>git ls-remote</code>.</small>
        </label>
      {/if}

      {#if msg?.where === 'form'}<div class="banner" class:err={!msg.ok} class:info={msg.ok}>{msg.text}</div>{/if}

      <div class="formfoot">
        <button class="btn" onclick={cancel}>Cancel</button>
        <button class="btn dark" disabled={!!busy} onclick={save}>Save source</button>
      </div>
    </div>
  {/if}
  </section>

  {:else if section === 'appearance'}
  <section class="settings-section">
    <div class="section-head"><div class="grow"><h2>Appearance</h2><p class="mut">Theme and fonts.</p></div></div>
    <div class="card setting-list">
      <div class="setting-row">
        <div class="setting-label"><b>Theme</b><small>System follows the Windows light/dark setting and switches with it.</small></div>
        <div class="seg theme-seg">
          {#each THEMES as t (t.id)}
            <button class:on={app.ws.theme === t.id} onclick={() => (app.ws.theme = t.id)}>{t.label}</button>
          {/each}
        </div>
      </div>
      <label class="setting-row">
        <span class="setting-label"><b>Interface font</b><small>Menus, labels and buttons.</small></span>
        <select bind:value={app.ws.uiFont}>{#each UI_FONTS as f (f.id)}<option value={f.id}>{f.label}</option>{/each}</select>
      </label>
      <label class="setting-row">
        <span class="setting-label"><b>Code font</b><small>Branches, paths and SHAs.</small></span>
        <select bind:value={app.ws.codeFont}>{#each CODE_FONTS as f (f.id)}<option value={f.id}>{f.label}</option>{/each}</select>
      </label>
    </div>
    <div class="preview">
      <div><b>application-feature</b> <span class="mut">demo-project · example-org · 3 changes</span></div>
      <div class="mono"><span class="t-branch">release/2.4</span> · <span class="t-tag">v2.4.1</span> · <span class="t-commit">a1b2c3d4</span> · C:\Dev\repos\example-org</div>
    </div>
  </section>

  {:else}
  <section class="settings-section">
    <div class="section-head"><div class="grow"><h2>Cloning</h2><p class="mut">Defaults used whenever you clone, fetch or switch a set.</p></div></div>
    <div class="card setting-list">
      <label class="setting-row">
        <span class="setting-label"><b>Shallow clone</b><small>Download only the latest commit; older history is not fetched.</small></span>
        <input type="checkbox" role="switch" bind:checked={app.ws.shallow} disabled={app.running} />
      </label>
      <label class="setting-row">
        <span class="setting-label"><b>Parallel clones</b><small>How many repositories are cloned at the same time (1–8).</small></span>
        <input class="parallel-input" type="number" min="1" max="8" step="1" value={app.ws.parallel} onchange={setParallel} disabled={app.running} />
      </label>
      <label class="setting-row">
        <span class="setting-label"><b>Existing folders</b><small>What to do when the destination folder already exists.</small></span>
        <select bind:value={app.ws.onExisting} disabled={app.running}>
          <option value="fetch">Fetch &amp; checkout</option>
          <option value="skip">Skip</option>
          <option value="reclone">Re-clone (keep backup)</option>
        </select>
      </label>
    </div>
    {#if app.running}<p class="hint">Locked while a Git operation is running.</p>{/if}
  </section>
  {/if}
    </div>
  </div>
</div>
