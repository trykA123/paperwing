<script lang="ts">
  import { confirm } from '../lib/confirm';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { app, uid } from '../lib/state.svelte';
  import { credentialLabel, credentialStoreName } from '../lib/state/credentials.svelte';
  import { api, type Source, type SourceKind } from '../lib/api';
  import Icon, { type IconName, type IconTone } from './Icon.svelte';
  import Alert from './Alert.svelte';
  import AppearanceSection from './settings/AppearanceSection.svelte';
  import CloningSection from './settings/CloningSection.svelte';
  import DiagnosticsSection from './settings/DiagnosticsSection.svelte';
  import { DiagnosticsStore, tauriDiagnostics } from '../lib/diagnostics.svelte';
  import SourceForm from './settings/SourceForm.svelte';

  type Section = 'sources' | 'appearance' | 'cloning' | 'diagnostics';
  const SECTIONS_BASE: { id: Section; label: string; icon: IconName; tone?: IconTone }[] = [
    { id: 'sources', label: 'Sources', icon: 'folder', tone: 'folder' },
    { id: 'appearance', label: 'Appearance', icon: 'theme', tone: 'brand' },
    { id: 'cloning', label: 'Cloning', icon: 'download', tone: 'branch' },
  ];
  const diagnostics = new DiagnosticsStore(tauriDiagnostics, (message, kind) => app.toast(message, kind));
  const SECTIONS = $derived(diagnostics.available
    ? [...SECTIONS_BASE, { id: 'diagnostics' as const, label: 'Diagnostics', icon: 'activity' as IconName }]
    : SECTIONS_BASE);
  void diagnostics.probe();
  let section = $state<Section>('sources');

  function setParallel(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const count = input.valueAsNumber;
    if (Number.isFinite(count)) app.ws.parallel = Math.max(1, Math.min(8, Math.round(count)));
    input.value = String(app.ws.parallel);
  }



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
  let busy = $state<'' | 'token' | 'orgs' | 'form'>('');
  let msg = $state<{ ok: boolean; text: string; where: 'token' | 'orgs' | 'form' } | null>(null);
  const storeName = $derived(credentialStoreName(app.platform.credentials?.backend ?? 'unsupported'));
  const tokenSaved = (id: string) => app.credentials.statuses[id]?.state === 'saved';
  let tokenAttempted = false;

  $effect(() => {
    const ids = new Set(app.sources.filter(source => source.kind !== 'manual').map(source => source.id));
    if (draft?.kind !== 'manual' && draft) ids.add(draft.id);
    for (const id of ids) void app.credentials.refresh(id);
  });

  const tokenUrl = $derived(
    draft && draft.kind !== 'manual' && draft.host
      ? `https://${draft.host}/settings/tokens/new?scopes=repo,read:org&description=Skein`
      : '',
  );

  function reset() {
    token = ''; orgInput = ''; myOrgs = []; msg = null; busy = ''; tokenAttempted = false;
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
    if (k === "manual" && draft.kind !== "manual" && !isNew) draft.credentialManaged = true;
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
    if (!draft || draft.kind === 'manual' || !token.trim()) return;
    const sourceId = draft.id, entered = token;
    tokenAttempted = true;
    draft.credentialManaged = true;
    await app.credentials.mutate(sourceId, entered.trim(), snapshot().host);
    if (draft?.id === sourceId && token === entered) token = '';
  }

  async function run(where: 'token' | 'orgs', task: () => Promise<void>) {
    msg = null;
    const missing = !draft?.host.trim() ? 'Enter the host first.'
      : !token.trim() && !tokenSaved(draft!.id) ? 'Paste a personal access token first.'
      : '';
    if (missing) { msg = { ok: false, text: missing, where }; return; }
    busy = where;
    try { await task(); } catch (e) { msg = { ok: false, text: String(e), where }; }
    busy = '';
  }

  const test = () => run('token', async () => {
    msg = { ok: true, text: `Connected as ${await api.testSource(snapshot(), token)}. The token works.`, where: 'token' };
  });
  const loadOrgs = () => run('orgs', async () => {
    myOrgs = await api.listUserOrgs(snapshot(), token);
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
    if (busy) return;
    busy = 'form';
    try {
      await saveToken();
      s.credentialManaged = draft?.credentialManaged;
      const sources = app.sources.map(source => source.id === s.id ? s : $state.snapshot(source) as Source);
      if (!sources.some(source => source.id === s.id)) sources.push(s);
      await app.saveSettings({ sources, workspace: $state.snapshot(app.ws) });
      app.credentials.invalidate(s.id);
      await app.credentials.synchronize(s.id);
      app.sources = sources;
      draft = null;
      app.toast(`Saved ${s.name}, loading repositories…`, 'success');
      await app.loadRepos(s, true);
    } catch (e) { msg = { ok: false, text: String(e), where: 'form' }; }
    finally { busy = ''; }
  }

  async function cancel() {
    if (busy) return;
    busy = 'form';
    try {
      if (isNew && draft && tokenAttempted) await app.credentials.mutate(draft.id);
      draft = null;
    } catch (e) { msg = { ok: false, text: `Could not remove the new source token. ${String(e)}`, where: 'form' }; }
    finally { busy = ''; }
  }

  async function remove(s: Source) {
    const ok = await confirm(`Remove "${s.name}"? ${s.kind !== "manual" || s.credentialManaged || app.platform.platform === "windows" ? `Its saved token is deleted from ${storeName}.` : "It does not use an API token."} Sets keep their repos.`,
      { title: 'Remove source', kind: 'warning', okLabel: 'Remove', destructive: true });
    if (!ok) return;
    if (busy) return;
    busy = 'form';
    try {
      if (s.kind !== 'manual' || s.credentialManaged || app.platform.platform === 'windows') await app.credentials.mutate(s.id);
      const sources = app.sources.filter(source => source.id !== s.id);
      await app.saveSettings({ sources: $state.snapshot(sources), workspace: $state.snapshot(app.ws) });
      app.sources = sources;
      app.credentials.invalidate(s.id);
    } catch (e) { app.toast(`Source retained. ${String(e)}`, 'error'); }
    finally { busy = ''; }
  }
</script>

<div class="settings">
  <header class="mh">
    <div class="grow"><div class="crumb">Skein</div><h1>Settings</h1></div>
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
      <div class="grow"><h2>Sources</h2><p class="mut">Where Skein finds repositories. Listing uses the host's API; cloning always uses your git + SSH setup.</p></div>
      {#if !draft}<button class="btn dark" onclick={startNew}><Icon name="plus" /> Add source</button>{/if}
    </div>

  {#if !app.sources.length && !draft}
    <Alert kind="info">No sources yet. Add GitHub, your GitHub Enterprise server, or manual repository URLs.</Alert>
  {/if}

  {#if app.sources.length}
    <div class="card">
      {#each app.sources as s (s.id)}
        <div class="srccard">
          <div class="grow">
            <div class="title"><b>{s.name}</b><span class="mut">{s.kind === 'manual' ? `${s.urls.length} URLs` : s.host}</span></div>
            {#if s.orgs.length}<div class="chips">{#each s.orgs as o}<span class="chip">{o}</span>{/each}</div>{/if}
            <small class="mut">
              {(app.repos[s.id] ?? []).length} repositories{#if app.staleRepos[s.id]} (cached){/if}
              {#if s.kind !== 'manual'} · <span class:okc={tokenSaved(s.id)} class:warn={!tokenSaved(s.id)}>{credentialLabel(app.credentials.statuses[s.id])}</span>{/if}
              {#if app.loadingRepos[s.id]} · <span class="spin"></span>{/if}
            </small>
            {#if s.kind !== 'manual' && app.credentials.statuses[s.id]?.reason}<p class="hint">{app.credentials.statuses[s.id].reason} <button class="link" onclick={() => app.credentials.refresh(s.id)}>Check again</button></p>{/if}
            {#if app.repoErrors[s.id]?.length}<div class="err" style="font-size:var(--fs-sm);margin-top:4px">{app.repoErrors[s.id].join(' · ')}</div>{/if}
          </div>
          {#if s.kind !== 'manual'}
            <button class="btn" disabled={!!busy || app.loadingRepos[s.id]} onclick={() => app.loadRepos(s, true)}><Icon name="refresh" /> Refresh</button>
          {/if}
          <button class="btn" disabled={!!busy} onclick={() => edit(s)}>Edit</button>
          <button class="btn icon-only" disabled={!!busy} title="Remove" onclick={() => remove(s)}><Icon name="trash" /></button>
        </div>
      {/each}
    </div>
  {/if}

  {#if draft}
    <SourceForm bind:draft bind:token bind:orgInput bind:urlsText kinds={KINDS}
      status={{ isNew, tokenSaved: tokenSaved(draft.id), credential: app.credentials.statuses[draft.id], storeName, tokenUrl, myOrgs, busy, msg }}
      actions={{ setKind, addOrg, test, loadOrgs, createToken: () => openUrl(tokenUrl), cancel, save }} />
  {/if}
  </section>

  {:else if section === 'appearance'}
  <AppearanceSection bind:theme={app.ws.theme} bind:uiFont={app.ws.uiFont} bind:codeFont={app.ws.codeFont} />
  {:else if section === 'diagnostics' && diagnostics.available}
  <DiagnosticsSection store={diagnostics} />
  {:else}
  <CloningSection bind:shallow={app.ws.shallow} bind:onExisting={app.ws.onExisting} parallel={app.ws.parallel} running={app.running} onparallel={setParallel} />
  {/if}
    </div>
  </div>
</div>
