<script lang="ts">
  import { app, RUNNING } from '../../lib/state.svelte';
  import type { SetItem } from '../../lib/api';
  import Icon from '../Icon.svelte';

  const SHOWN = 8;
  let { items }: { items: SetItem[] } = $props();

  function parts(item: SetItem) {
    const segments = app.segments(item);
    return { dir: segments.length > 1 ? `${segments.slice(0, -1).join(app.platform.separator)}${app.platform.separator}` : '', leaf: segments.at(-1) ?? '' };
  }
  function tag(item: SetItem): { text: string; tone: 'new' | 'ok' | 'warn' | 'err' } {
    if (app.hasClash(item)) return { text: 'name clash', tone: 'err' };
    if (app.refState(item) === 'missing') return { text: 'ref not found', tone: 'warn' };
    if (app.exists[app.dest(item)]) return { text: `exists \u00b7 ${EXISTS[app.ws.onExisting]}`, tone: app.ws.onExisting === 'reclone' ? 'warn' : 'ok' };
    return { text: 'new', tone: 'new' };
  }
  const EXISTS = { fetch: 'fetch', skip: 'skip', reclone: 're-clone' } as const;

  function dot(i: SetItem) {
    const j = app.jobs[i.id];
    if (j) return RUNNING.includes(j.phase) ? 'd-run' : j.phase === 'queued' ? '' : `d-${j.phase}`;
    if (app.hasClash(i)) return 'd-failed';
    return app.refState(i) === 'missing' ? 'd-warn' : '';
  }
</script>

    <div class="rsec">
      <h3 class="hrow"><span>Will be created <small>{items.length}</small></span></h3>
      {#if items.length}
        <ul class="plan">
          {#each items.slice(0, SHOWN) as item (item.id)}
            {@const place = parts(item)}
            {@const state = tag(item)}
            <li class="plan-row">
              <span class="dot {dot(item)}"></span>
              <span class="plan-path" title={app.dest(item)}><span class="mut">{place.dir}</span><b>{place.leaf}</b>{#if item.folder}<span class="mut"> ({item.name})</span>{/if}</span>
              <span class="plan-ref t-{item.ref.type}"><Icon name={item.ref.type} size={12} />{app.refLabel(item)}</span>
              <span class="plan-tag {state.tone}">{state.text}</span>
            </li>
          {/each}
        </ul>
        {#if items.length > SHOWN}<p class="mut plan-more">and {items.length - SHOWN} more</p>{/if}
      {:else}
        <p class="mut">Nothing to clone. Every repository of this set is on disk.</p>
      {/if}
    </div>

    <div class="rsec">
      <h3>Options</h3>
      <div class="opt-summary">
        <span>{app.ws.shallow ? 'Shallow' : 'Full history'} · {app.ws.parallel} parallel · existing: {EXISTS[app.ws.onExisting]}</span>
        <button class="link" onclick={() => app.openView({ kind: 'settings' })}>Change in Settings</button>
      </div>
    </div>
