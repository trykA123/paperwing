<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { ago, app, matches, type RefsEntry } from '../lib/state.svelte';
  import type { Commit, Ref, RefKind, SetItem } from '../lib/api';
  import Icon from './Icon.svelte';

  let { items, anchor, onclose, onpick }: {
    items: SetItem[]; anchor: DOMRect; onclose: () => void; onpick: (ref: Ref) => void;
  } = $props();

  /** One row of a `git log --graph`-style lane layout. */
  type Lane = { col: number; top: (string | null)[]; bottom: (string | null)[]; merges: number[]; forks: number[] };
  type Row =
    | { k: 'sec'; title: string; n?: number }
    | { k: 'grp'; name: string; n: number; open: boolean }
    | { k: 'ref'; type: RefKind; name: string; cov: number; meta?: string; short?: boolean; idx: number; c?: Commit; g?: Lane }
    | { k: 'note'; text: string; err?: boolean };

  const W = 600, ROW_H = 46, LANE_W = 14, MAX_LANES = 8;
  const LANE_COLORS = ['#6a48f5', '#10b981', '#f59e0b', '#ef4444', '#06b6d4', '#ec4899', '#84cc16', '#8b5cf6'];
  // Open on whichever side of the clicked button has more room, sized to fit, so it stays attached to the row.
  const MAX_H = 620, MIN_H = 320, GAP = 6, EDGE = 8;
  const below = untrack(() => innerHeight - anchor.bottom - GAP - EDGE);
  const above = untrack(() => anchor.top - GAP - EDGE);
  const openBelow = below >= Math.min(MAX_H, MIN_H) || below >= above;
  const H = Math.max(Math.min(MAX_H, openBelow ? below : above), Math.min(MIN_H, innerHeight - 2 * EDGE));
  const total = untrack(() => items.length);
  const single = total === 1;
  const current = untrack(() => (single ? items[0].ref : null));
  const x = untrack(() => Math.max(EDGE, Math.min(anchor.left, innerWidth - W - EDGE)));
  const y = untrack(() => Math.max(EDGE, Math.min(innerHeight - H - EDGE, openBelow ? anchor.bottom + GAP : anchor.top - GAP - H)));

  let tab = $state<RefKind>(current?.type ?? 'branch');
  let commitView = $state<'graph' | 'list'>('graph');
  let q = $state('');
  let active = $state(0);
  let open = $state<Record<string, boolean>>({ release: true });
  let pop: HTMLDivElement;
  let input: HTMLInputElement;
  let listEl: HTMLDivElement;

  onMount(() => {
    input.focus();
    app.ensureRefs(items.map(i => i.url));
  });
  $effect(() => {
    if (single && tab === 'commit') untrack(() => app.ensureCommits(items[0]));
  });

  const entries = $derived(items.map(i => app.refs[i.url]));
  const loaded = $derived(entries.filter(e => e && !e.loading).length);
  const loading = $derived(loaded < total);
  const failed = $derived(items.filter(i => app.refs[i.url]?.error));
  const commitsEntry = $derived(single ? app.commits[items[0].repoId] : undefined);
  const commits = $derived(Array.isArray(commitsEntry) ? commitsEntry : []);

  function tally(get: (e: RefsEntry) => string[]): [string, number][] {
    const m = new Map<string, number>();
    for (const e of entries) if (e && !e.loading && !e.error) for (const n of get(e)) m.set(n, (m.get(n) ?? 0) + 1);
    return [...m.entries()];
  }
  const br = $derived(tally(e => e.branches));
  const tg = $derived(tally(e => e.tags));
  const byCov = (a: [string, number][]) => (total > 1 ? [...a].sort((p, q2) => q2[1] - p[1]) : a);
  const rank = (g: string) => ({ release: 0, feature: 1, bugfix: 2 } as Record<string, number>)[g] ?? 3;
  const firstLine = (s: string) => s.split('\n')[0];
  function refLabel(type: RefKind, name: string) {
    if (type === 'commit') return name;
    for (const entry of entries) {
      const names = type === 'branch' ? entry?.branches : entry?.tags;
      const labels = type === 'branch' ? entry?.branchLabels : entry?.tagLabels;
      const index = names?.indexOf(name) ?? -1;
      if (index >= 0) return labels?.[index] ?? name;
    }
    return name;
  }

  function groupLabel(name: string) {
    const branch = br.find(([value]) => value.startsWith(`${name}/`))?.[0];
    return branch ? refLabel('branch', branch).split('/')[0] : name;
  }


  // Branch/tag names pointing at each commit, to label graph rows.
  const tips = $derived.by(() => {
    const m = new Map<string, { type: RefKind; name: string }[]>();
    const e = single ? entries[0] : undefined;
    if (!e || e.loading || e.error) return m;
    const add = (sha: string | undefined, type: RefKind, name: string) => {
      if (!sha) return;
      if (!m.has(sha)) m.set(sha, []);
      m.get(sha)!.push({ type, name: refLabel(type, name) });
    };
    e.branches.forEach((n, i) => add(e.branchShas?.[i], 'branch', n));
    e.tags.forEach((n, i) => add(e.tagShas?.[i], 'tag', n));
    return m;
  });

  function layout(list: Commit[]): Lane[] {
    const lanes: (string | null)[] = [];
    const free = () => {
      const i = lanes.indexOf(null);
      return i >= 0 ? i : lanes.push(null) - 1;
    };
    return list.map(c => {
      let col = lanes.indexOf(c.sha);
      if (col < 0) col = free();
      const top = lanes.slice();
      const merges = top.flatMap((s, i) => (s === c.sha && i !== col ? [i] : []));
      for (const i of merges) lanes[i] = null;
      lanes[col] = c.parents[0] ?? null;
      const forks = c.parents.slice(1).map(p => {
        let j = lanes.indexOf(p);
        if (j < 0) { j = free(); lanes[j] = p; }
        return j;
      });
      while (lanes.length && lanes[lanes.length - 1] === null) lanes.pop();
      return { col, top, bottom: lanes.slice(), merges, forks };
    });
  }
  const lanes = $derived(layout(commits));
  const graphW = $derived(Math.min(MAX_LANES, Math.max(1, ...lanes.map(l => Math.max(l.top.length, l.bottom.length, l.col + 1)))) * LANE_W + 6);
  const lx = (i: number) => 7 + Math.min(i, MAX_LANES - 1) * LANE_W;
  const lc = (i: number) => LANE_COLORS[i % LANE_COLORS.length];

  const rows = $derived.by(() => {
    const out: Row[] = [];
    let idx = 0;
    const ref = (type: RefKind, name: string, cov = 1, meta?: string, short = false) =>
      out.push({ k: 'ref', type, name, cov, meta, short, idx: idx++ });
    const section = (title: string, list: [string, number][], type: RefKind, short = false) => {
      if (!list.length) return;
      out.push({ k: 'sec', title, n: list.length });
      list.forEach(([n, c]) => ref(type, n, c, undefined, short));
    };
    const query = q.trim();

    if (tab === 'branch') {
      if (query) {
        section('Matching branches', byCov(br.filter(([n]) => matches(n, query))), 'branch');
        if (!out.length && !loading) out.push({ k: 'note', text: 'No branch matches.' });
        return out;
      }
      const tops: [string, number][] = [];
      const groups = new Map<string, [string, number][]>();
      for (const e of br) {
        const s = e[0].indexOf('/');
        if (s < 0) tops.push(e);
        else {
          const g = e[0].slice(0, s);
          if (!groups.has(g)) groups.set(g, []);
          groups.get(g)!.push(e);
        }
      }
      section('Default', byCov(tops), 'branch');
      const order = [...groups.keys()].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));
      if (order.length) out.push({ k: 'sec', title: 'Groups' });
      for (const g of order) {
        const list = groups.get(g)!;
        const isOpen = !!open[g];
        out.push({ k: 'grp', name: g, n: list.length, open: isOpen });
        if (!isOpen) continue;
        const sorted = g === 'release' ? [...list].reverse() : list;
        byCov(sorted).forEach(([n, c]) => ref('branch', n, c, undefined, true));
      }
      return out;
    }

    if (tab === 'tag') {
      section(query ? 'Matching tags' : 'Tags, newest first', byCov(tg.filter(([n]) => matches(n, query))), 'tag');
      if (!out.length && !loading) out.push({ k: 'note', text: query ? 'No tag matches.' : 'This repository has no tags.' });
      return out;
    }

    const lower = query.toLowerCase();
    if (/^[0-9a-f]{7,40}$/.test(lower) && !commits.some(c => c.sha.startsWith(lower))) {
      out.push({ k: 'sec', title: 'Commit by SHA' });
      ref('commit', lower, 1, 'Verified while cloning');
    }
    const cm = commits.filter(c => c.sha.startsWith(lower) || matches(`${c.message} ${c.author}`, query));
    if (cm.length) {
      // Lanes only make sense for the unfiltered, contiguous history.
      const graph = commitView === 'graph' && !query;
      out.push({ k: 'sec', title: graph ? 'History' : 'Recent commits', n: cm.length });
      cm.forEach((c, i) => out.push({ k: 'ref', type: 'commit', name: c.sha, cov: 1, idx: idx++, c, g: graph ? lanes[i] : undefined }));
    }
    if (commitsEntry && typeof commitsEntry === 'object' && 'error' in commitsEntry)
      out.push({ k: 'note', text: commitsEntry.error, err: true });
    if (!out.length)
      out.push({ k: 'note', text: commitsEntry === 'loading' ? 'Loading recent commits…' : 'Paste a commit SHA (7–40 hex characters).' });
    return out;
  });
  const pickable = $derived(rows.filter(r => r.k === 'ref').length);

  function setTab(t: RefKind) {
    tab = t;
    active = 0;
    input.focus();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      active = Math.max(0, Math.min(pickable - 1, active + (e.key === 'ArrowDown' ? 1 : -1)));
      listEl.querySelector(`[data-idx="${active}"]`)?.scrollIntoView({ block: 'nearest' });
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const r = rows.find(r => r.k === 'ref' && r.idx === active);
      if (r?.k === 'ref') onpick({ type: r.type, name: r.name });
    } else if (e.key === 'Escape') {
      onclose();
    }
  }
</script>

<svelte:window onmousedown={e => { if (pop && !pop.contains(e.target as Node)) onclose(); }} />

<div class="pop" bind:this={pop} style:left="{x}px" style:top="{y}px" style:height="{H}px">
  <div class="ptitle">
    Checkout for <b>{single ? `${items[0].org}/${items[0].name}` : `${total} selected repos`}</b>{#if !single} · applied where it exists{/if}
  </div>
  <div class="ptabs">
    <button class:on={tab === 'branch'} onclick={() => setTab('branch')}>Branches<span>{br.length}</span></button>
    <button class:on={tab === 'tag'} onclick={() => setTab('tag')}>Tags<span>{tg.length}</span></button>
    {#if single}<button class:on={tab === 'commit'} onclick={() => setTab('commit')}>Commit<span>{commits.length || ''}</span></button>{/if}
    <span class="grow"></span>
    {#if tab === 'commit'}
      <div class="seg mini-seg">
        <button class:on={commitView === 'graph'} onclick={() => { commitView = 'graph'; input.focus(); }}>Graph</button>
        <button class:on={commitView === 'list'} onclick={() => { commitView = 'list'; input.focus(); }}>List</button>
      </div>
    {/if}
    {#if loading}<span class="mut" style="font-size:var(--fs-sm)">{total > 1 ? `${loaded}/${total}` : ''}</span><span class="spin" title="Asking the remote…"></span>{/if}
  </div>
  <div class="pinput">
    <Icon name="search" />
    <input bind:this={input} bind:value={q} oninput={() => (active = 0)} onkeydown={onKey} autocomplete="off" spellcheck="false"
      placeholder={tab === 'branch' ? 'Filter branches…' : tab === 'tag' ? 'Filter tags…' : 'Paste a SHA or search messages…'} />
  </div>
  <div class="plist" bind:this={listEl}>
    {#if loading && tab !== 'commit' && !br.length && !tg.length}<div class="pnote">Asking the remote{total > 1 ? 's' : ''}…</div>{/if}
    {#each failed.slice(0, 3) as f (f.id)}<div class="pnote err">{f.name}: {app.refs[f.url]?.error}</div>{/each}
    {#each rows as r, i (i)}
      {#if r.k === 'sec'}
        <div class="psec">{r.title}{#if r.n !== undefined}<span>{r.n}</span>{/if}</div>
      {:else if r.k === 'grp'}
        <button class="pgrp" onclick={() => { open[r.name] = !open[r.name]; input.focus(); }}>
          <span class="chev">{r.open ? '▾' : '▸'}</span>{groupLabel(r.name)}/<span class="n">{r.n}</span>
        </button>
      {:else if r.k === 'note'}
        <div class="pnote" class:err={r.err}>{r.text}</div>
      {:else if r.c}
        {@const isCur = !!current && current.type === 'commit' && r.name.startsWith(current.name)}
        {@const g = r.g}
        {@const mid = ROW_H / 2}
        {@const labels = tips.get(r.name) ?? []}
        <button class="pi cm" class:act={r.idx === active} data-idx={r.idx} style:height="{ROW_H}px" onmousemove={() => (active = r.idx)}
          onclick={() => onpick({ type: 'commit', name: r.name })} title={r.c.message}>
          {#if g}
            <svg class="graph" width={graphW} height={ROW_H} aria-hidden="true">
              {#each g.top as s, i}
                {#if s && i !== g.col && s !== r.name}<line x1={lx(i)} y1="0" x2={lx(i)} y2={ROW_H} stroke={lc(i)} />{/if}
              {/each}
              {#if g.top[g.col] === r.name}<line x1={lx(g.col)} y1="0" x2={lx(g.col)} y2={mid} stroke={lc(g.col)} />{/if}
              {#each g.merges as i}<path d="M{lx(i)} 0 C {lx(i)} {mid / 2}, {lx(g.col)} {mid / 2}, {lx(g.col)} {mid}" stroke={lc(i)} />{/each}
              {#if g.bottom[g.col]}<line x1={lx(g.col)} y1={mid} x2={lx(g.col)} y2={ROW_H} stroke={lc(g.col)} />{/if}
              {#each g.forks as j}<path d="M{lx(g.col)} {mid} C {lx(g.col)} {mid + mid / 2}, {lx(j)} {mid + mid / 2}, {lx(j)} {ROW_H}" stroke={lc(j)} />{/each}
              <circle cx={lx(g.col)} cy={mid} r={r.c.parents.length > 1 ? 3 : 4} fill={r.c.parents.length > 1 ? 'var(--panel)' : lc(g.col)} stroke={lc(g.col)} />
            </svg>
          {:else}
            <span class="t-commit cmi"><Icon name="commit" /></span>
          {/if}
          <span class="cmt">
            <span class="msg">
              {#each labels.slice(0, 3) as t (t.type + t.name)}<span class="clbl l-{t.type}">{t.name}</span>{/each}{#if labels.length > 3}<span class="clbl" title={labels.slice(3).map(t => t.name).join(', ')}>+{labels.length - 3}</span>{/if}{firstLine(r.c.message)}
            </span>
            <span class="sub"><span class="sha">{r.name.slice(0, 8)}</span> · {r.c.author} · {ago(r.c.date)}</span>
          </span>
          {#if isCur}<span class="curm">current</span>{/if}
        </button>
      {:else}
        {@const isCur = !!current && current.type === r.type && current.name === r.name}
        {@const label = refLabel(r.type, r.name)}
        {@const slash = label.indexOf('/')}
        <button class="pi" class:act={r.idx === active} data-idx={r.idx} onmousemove={() => (active = r.idx)}
          onclick={() => onpick({ type: r.type, name: r.name })}>
          <span class="t-{r.type}"><Icon name={r.type} /></span>
          <span class="nm">{#if r.short && slash > 0}<span class="pre">{label.slice(0, slash + 1)}</span>{label.slice(slash + 1)}{:else}{r.type === 'commit' ? r.name.slice(0, 8) : label}{/if}</span>
          {#if r.meta}<span class="meta">{r.meta}</span>{/if}
          {#if isCur}<span class="curm">current</span>{:else if total > 1}<span class="cov" class:part={r.cov < total}>{r.cov}/{total}</span>{/if}
        </button>
      {/if}
    {/each}
  </div>
  <div class="pfoot"><span><kbd>↑</kbd><kbd>↓</kbd> move</span><span><kbd>Enter</kbd> select</span><span><kbd>Esc</kbd> close</span></div>
</div>
