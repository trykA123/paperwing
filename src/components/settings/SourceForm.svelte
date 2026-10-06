<script lang="ts">
  import type { Source, SourceKind, CredentialStatus } from '../../lib/api';
  import Icon from '../Icon.svelte';
  import Alert from '../Alert.svelte';

  let { draft = $bindable(), token = $bindable(), orgInput = $bindable(), urlsText = $bindable(), kinds, status, actions }: {
    draft: Source; token: string; orgInput: string; urlsText: string;
    kinds: { id: SourceKind; title: string; desc: string }[];
    status: { isNew: boolean; tokenSaved: boolean; credential?: CredentialStatus; storeName: string; tokenUrl: string; myOrgs: string[]; busy: '' | 'token' | 'orgs' | 'form'; msg: { ok: boolean; text: string; where: 'token' | 'orgs' | 'form' } | null };
    actions: { setKind: (kind: SourceKind) => void; addOrg: (name?: string) => void; test: () => void; loadOrgs: () => void; createToken: () => void; cancel: () => void; save: () => void };
  } = $props();
</script>

<div class="card form">
      <h2>{status.isNew ? 'Add source' : `Edit ${draft.name}`}</h2>
      <div class="kinds">
        {#each kinds as k}
          <button class="kind" class:on={draft.kind === k.id} disabled={!!status.busy} onclick={() => actions.setKind(k.id)}><b>{k.title}</b><small>{k.desc}</small></button>
        {/each}
      </div>

      <div class="grid2">
        <label class="fld"><span>Display name</span><input disabled={!!status.busy} bind:value={draft.name} /></label>
        {#if draft.kind === 'ghe'}
          <label class="fld"><span>Host</span><input disabled={!!status.busy} bind:value={draft.host} placeholder="git.example.com" spellcheck="false" /></label>
        {/if}
      </div>

      {#if draft.kind !== 'manual'}
        <div class="fld">
          <span>Personal access token {#if status.tokenSaved}<em class="okc">✓ saved in {status.storeName}</em>{/if}</span>
          <div class="row">
            <input type="password" aria-label="Personal access token" disabled={!!status.busy} bind:value={token} autocomplete="off" spellcheck="false"
              placeholder={status.tokenSaved ? 'Leave empty to keep the saved token' : 'Paste a token (classic) here'} />
            <button class="btn" disabled={!!status.busy} onclick={actions.test} title="Asks the server who you are, using this host and token">
              {#if status.busy === 'token'}<span class="spin"></span>{:else}<Icon name="check" />{/if} Test connection
            </button>
            {#if status.tokenUrl}<button class="btn" onclick={actions.createToken}>Create token ↗</button>{/if}
          </div>
          <small class="hint">Needs the <code>repo</code> and <code>read:org</code> scopes. It is only used to list repositories and commits and is never written to the settings file. <b>Test connection</b> checks that the host and token work.</small>
          {#if status.credential?.reason && !(status.msg && !status.msg.ok)}<Alert kind="err" role="status">{status.credential.reason}</Alert>{/if}
          {#if status.msg?.where === 'token'}<Alert kind={status.msg.ok ? 'info' : 'err'}>{status.msg.text}</Alert>{/if}
        </div>

        <div class="fld">
          <span>Organizations or users
            <button class="btn small" disabled={!!status.busy} onclick={actions.loadOrgs} title="Lists the organizations your account belongs to">{#if status.busy === 'orgs'}<span class="spin"></span>{/if}Load my organizations</button>
          </span>
          <div class="chipbox">
            {#each draft.orgs as o (o)}
              <span class="chip">{o}<button title="Remove" onclick={() => (draft!.orgs = draft!.orgs.filter(x => x !== o))}>×</button></span>
            {/each}
            <input bind:value={orgInput} placeholder={draft.orgs.length ? 'Add another…' : 'Type an org name and press Enter'} spellcheck="false"
              onkeydown={e => { if (e.key === 'Enter' || e.key === ',') { e.preventDefault(); actions.addOrg(); } }} onblur={() => actions.addOrg()} />
          </div>
          {#if status.myOrgs.length}
            <div class="chips">
              <span class="hint">Your organizations:</span>
              {#each status.myOrgs as o (o)}
                {@const on = draft.orgs.includes(o)}
                <button class="chip pick" class:on onclick={() => (on ? (draft!.orgs = draft!.orgs.filter(x => x !== o)) : actions.addOrg(o))}>{on ? '✓ ' : '+ '}{o}</button>
              {/each}
            </div>
          {/if}
          <small class="hint">Repos from these organizations appear in the sidebar. Use the name from the URL, e.g. <code>example-org</code> in <code>github.com/example-org/demo-project</code>. <b>Load my organizations</b> lists the ones your account belongs to so you can click them.</small>
          {#if status.msg?.where === 'orgs'}<Alert kind={status.msg.ok ? 'info' : 'err'}>{status.msg.text}</Alert>{/if}
        </div>
      {:else}
        <label class="fld">
          <span>Repository URLs, one per line</span>
          <textarea rows="8" bind:value={urlsText} spellcheck="false"
            placeholder={'git@github.com:example-org/demo-project.git\nssh://git@git.example.com:7999/projects/demo-project.git'}></textarea>
          <small class="hint">For hosts Skein cannot list. The folder/org name comes from the URL path; branches and tags are read with <code>git ls-remote</code>.</small>
        </label>
      {/if}

      {#if status.msg?.where === 'form'}<Alert kind={status.msg.ok ? 'info' : 'err'}>{status.msg.text}</Alert>{/if}

      <div class="formfoot">
        <button class="btn" disabled={!!status.busy} onclick={actions.cancel}>Cancel</button>
        <button class="btn dark" disabled={!!status.busy} onclick={actions.save}>Save source</button>
      </div>
    </div>
