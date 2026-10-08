<script lang="ts">
  import { untrack } from 'svelte';
  import { benchmarkEnabled, benchmarkTimer } from '../lib/benchmark';
  import { ask, confirm } from '../lib/confirm';
  import { api, type EditFile } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import type { CompareState } from '../lib/compare.svelte';
  import { tabId, type View } from '../lib/workspace';
  import { commands, execute } from '../lib/commands';
  import { compareFullscreen } from '../lib/compare-fullscreen';
  import { createCompareEditor, currentTheme, LARGE_FILE_NOTICE, loadEngine, type CompareEditor, type EngineKind, type Side } from '../lib/editor';
  import { openSides } from '../lib/compare-sides';
  import { fileSignature } from '../lib/file-signature';
  import { reopenSide } from '../lib/side-tickets';
  import { languageLabel, resolveLanguage, extensionOf } from '../lib/languages';
  import { verifyReadonly } from '../lib/readonly-benchmark';
  import type { TextFormat } from '../lib/text-format';
  import { countChanges, describeCopy, type ChangeCounts } from '../lib/change-summary';
  import ConfirmBar from './file-compare/ConfirmBar.svelte';
  import Footer from './file-compare/Footer.svelte';
  import Header from './file-compare/Header.svelte';
  import MoreMenu from './file-compare/MoreMenu.svelte';
  import PaneHeads from './file-compare/PaneHeads.svelte';

  let { view, comparison, active }: { view: Extract<View, { kind: 'fileDiff' }>; comparison: CompareState; active: boolean } = $props();
  let host: HTMLDivElement;
  let editor = $state.raw<CompareEditor | null>(null);
  let loading = $state(false), error = $state(''), fallback = $state(''), busy = $state(false), notice = $state('');
  let inline = $state(false), hideSame = $state(false), ignoreWhitespace = $state(false);
  let dirty = $state([false, false]), hunkIndex = $state(-1), changeCount = $state(0), computing = $state(true);
  let counts = $state<ChangeCounts>({ add: 0, rem: 0, chg: 0 }), pendingCopy = $state<'left' | 'right' | null>(null);
  let copyOpener: Element | null = null;
  let formats = $state<TextFormat[]>([]);
  let sniffed = $state(''), override = $state<string | null>(null);
  let tickets = $state<(EditFile | null)[]>([]);
  let readOnlyReasons = $state<(string | null)[]>([]);
  let undoIds = $state<string[]>([]);
  let ownedTickets = new Set<string>();
  let revision = 0;
  let readonlyMeasured = false;
  const sides: Side[] = ['left', 'right'];
  const snapshot = $derived(comparison.snapshot);
  const stale = $derived(!snapshot || snapshot.id !== view.sessionId || snapshot.generation !== view.generation);
  const file = $derived(comparison.files.find(file => file.id === view.fileId));
  const fileKey = $derived(fileSignature(file));
  const identity = $derived(tabId(view, ''));
  const writable = $derived([snapshot?.left.endpoint, snapshot?.right.endpoint].map(endpoint => app.platform.platform !== 'linux' || !!endpoint && app.endpointCapability(endpoint, 'edit').supported));
  const canCopyLeft = $derived(!stale && !busy && !computing && !!tickets[0] && writable[0] && !!file?.right && changeCount > 0);
  const canCopyRight = $derived(!stale && !busy && !computing && !!tickets[1] && writable[1] && !!file?.left && changeCount > 0);
  const extension = $derived(extensionOf(view.path));
  const language = $derived(override ?? resolveLanguage(view.path, app.ws.languageMap, sniffed));
  const remembered = $derived(!!extension && !!app.ws.languageMap?.[extension]);
  const editorCommands = $derived(commands());
  function rememberLanguage(id: string | null) {
    const map = { ...app.ws.languageMap };
    if (id) map[extension] = id; else delete map[extension];
    app.ws.languageMap = map; override = null;
  }
  function pickLanguage(id: string) { if (remembered) rememberLanguage(id); else override = id; }
  function command(id: string) { return editorCommands.find(command => command.id === id)!; }
  $effect(() => {
    const tabIdentity = identity;
    app.copyActions[tabIdentity] = {
      leftReason: snapshot ? app.endpointCapability(snapshot.left.endpoint, 'copy').reason : null,
      rightReason: snapshot ? app.endpointCapability(snapshot.right.endpoint, 'copy').reason : null,
      left: !stale && !busy && !!snapshot && app.endpointCapability(snapshot.left.endpoint, 'copy').supported && snapshot?.left.endpoint.reference.kind === 'workingTree' && file?.right?.kind === 'file' && !file.right.reason,
      right: !stale && !busy && !!snapshot && app.endpointCapability(snapshot.right.endpoint, 'copy').supported && snapshot?.right.endpoint.reference.kind === 'workingTree' && file?.left?.kind === 'file' && !file.left.reason,
      copy: side => app.requestCopy(view.comparisonId, view.fileId, side),
    };
    return () => { delete app.copyActions[tabIdentity]; };
  });
  function refreshDirty() { dirty = sides.map((side, index) => !!tickets[index] && !!editor?.isDirty(side)); }
  function refreshChanges() {
    const index = editor?.currentChange() ?? -1;
    if (index !== hunkIndex) pendingCopy = null;
    changeCount = editor?.changes().length ?? 0; hunkIndex = index; counts = countChanges(editor?.changes() ?? []);
  }

  async function save(side?: number) {
    if (busy || stale || !editor || !app.fileCapability('edit').supported) return;
    busy = true; error = '';
    try {
      for (const index of side === undefined ? [0, 1] : [side]) {
        if (!dirty[index] || !tickets[index]) continue;
        if (!writable[index]) throw new Error(app.endpointCapability(index === 0 ? snapshot!.left.endpoint : snapshot!.right.endpoint, 'edit').reason ?? 'Editing is unavailable for this root.');
        const saved = editor.snapshot(sides[index]!);
        const record = await api.fileSave(tickets[index]!.ticket, saved.bytes);
        editor.markSaved(sides[index]!, saved);
        undoIds.push(record.id); undoIds = undoIds.slice(-32);
        if (record.warning) app.toast(record.warning, 'warn');
      }
      app.toast('Saved', 'success');
    } catch (reason) { error = String(reason); }
    finally { refreshDirty(); busy = false; }
  }

  async function guard() {
    if (busy) return false;
    if (!dirty.some(Boolean)) return true;
    if (await ask(`Save changes to ${view.path}?`, { title: 'Unsaved changes', kind: 'warning', okLabel: 'Save', cancelLabel: 'Other options' })) {
      await save(); return !dirty.some(Boolean);
    }
    if (!await confirm(`Discard unsaved buffer changes to ${view.path}?`, { title: 'Unsaved changes', kind: 'warning', okLabel: 'Discard', cancelLabel: 'Keep editing', destructive: true })) return false;
    for (const side of sides) editor?.revert(side);
    dirty = [false, false]; return true;
  }

  function next(direction: number) {
    if (!editor || computing || !changeCount) return;
    if (direction > 0 && editor.currentChange() < 0) editor.goToChange(1);
    editor.goToChange(direction > 0 ? 1 : -1); refreshChanges();
  }

  function copy(side: 'left' | 'right') {
    if (side === 'left' ? !canCopyLeft : !canCopyRight) return;
    editor?.copyChange(side === 'left' ? 'right' : 'left', side, Math.max(0, hunkIndex));
  }

  function askCopy(side: 'left' | 'right') {
    if (side === 'left' ? !canCopyLeft : !canCopyRight) return;
    copyOpener = document.activeElement; pendingCopy = side;
  }

  function closeCopyBar(apply: boolean) {
    const side = pendingCopy;
    pendingCopy = null;
    if (apply && side) copy(side);
    if (copyOpener instanceof HTMLElement && copyOpener.isConnected) copyOpener.focus();
  }

  const copyMessage = $derived.by(() => {
    const change = pendingCopy && editor?.changes()[Math.max(0, hunkIndex)];
    return pendingCopy && change ? describeCopy(change, pendingCopy, Math.max(0, hunkIndex), changeCount) : '';
  });

  async function undoSave() {
    if (!app.capability('recovery').supported || !undoIds.length || !await guard()) return;
    const id = undoIds.at(-1); if (!id || !editor) return;
    busy = true; error = '';
    try {
      await api.recoveryUndo(id); undoIds = undoIds.filter(record => record !== id);
      for (const index of [0, 1]) {
        const previous = tickets[index];
        if (!previous) continue;
        ownedTickets.delete(previous.ticket);
        const reopened = await reopenSide(view, sides[index]!, previous);
        tickets[index] = reopened.ticket; readOnlyReasons[index] = reopened.reason;
        if (reopened.ticket) ownedTickets.add(reopened.ticket.ticket);
        editor.setContent(sides[index]!, reopened.content);
        formats[index] = editor.format(sides[index]!);
      }
      dirty = [false, false]; app.toast('Filesystem operation undone', 'success');
    } catch (reason) { error = String(reason); } finally { busy = false; }
  }

  $effect(() => {
    const sessionId = view.sessionId, generation = view.generation, fileId = view.fileId, path = view.path;
    const tabIdentity = identity;
    if (!host || stale || !fileKey) return;
    const target = untrack(() => ({ file: file!, endpoints: [snapshot!.left.endpoint, snapshot!.right.endpoint] }));
    const current = ++revision; let disposed = false;
    const handles: { dispose: () => void }[] = [];
    const opened = new Set<string>(); ownedTickets = opened;
    loading = true; fallback = ''; error = ''; notice = ''; computing = true;
    (async () => {
      const result = await openSides({ sessionId, generation, fileId, file: target.file, endpoints: target.endpoints, opened, alive: () => !disposed && current === revision });
      if (result.status === 'cancelled') return;
      if (result.status === 'unsupported') { fallback = result.message; return; }
      const { sides: opening } = result;
      tickets = opening.tickets; readOnlyReasons = opening.reasons;
      if (opening.kind === 'viewer') notice = LARGE_FILE_NOTICE;
      const finishImport = benchmarkTimer('editor.import');
      await loadEngine(opening.kind);
      finishImport();
      sniffed = (opening.contents[0]!.format.text || opening.contents[1]!.format.text).slice(0, 2000);
      const finishConstruct = benchmarkTimer('editor.construct');
      const finishDiff = benchmarkTimer('editor.diff');
      const created = await createCompareEditor({ host, kind: opening.kind, left: opening.contents[0]!, right: opening.contents[1]!,
        settings: { layout: inline ? 'inline' : 'sideBySide', theme: currentTheme(), language, hideUnchanged: hideSame, ignoreWhitespace,
          readOnly: { left: !tickets[0], right: !tickets[1] }, locked: false } });
      if (disposed || current !== revision) { created.dispose(); return; }
      handles.push(created);
      editor = created; formats = sides.map(side => created.format(side));
      finishConstruct(); finishDiff();
      handles.push({ dispose: created.on(event => { if (event.type === 'text') refreshDirty(); else refreshChanges(); }) });
      refreshDirty(); refreshChanges(); computing = false;
      if (benchmarkEnabled && app.readonlyBenchmark && !readonlyMeasured) {
        readonlyMeasured = true;
        void verifyReadonly({ sessionId, generation, fileId }, { editor: created, host, tickets, snapshot: snapshot!, files: comparison.files, platform: app.platform.platform })
          .catch(reason => { error = String(reason); });
      }
      app.bufferGuards.set(tabIdentity, guard); comparison.editorGuards.set(tabIdentity, guard);
    })().catch(reason => {
      for (const ticket of opened) void api.editClose(ticket).catch(() => {}); opened.clear();
      if (!disposed) { fallback = 'This content cannot be edited.'; error = String(reason); }
    })
      .finally(() => { if (!disposed) loading = false; });
    return () => {
      disposed = true; ++revision; for (const handle of handles.reverse()) handle.dispose();
      for (const ticket of opened) void api.editClose(ticket).catch(() => {}); opened.clear();
      app.bufferGuards.delete(tabIdentity); comparison.editorGuards.delete(tabIdentity); delete app.editorActions[tabIdentity]; editor = null;
    };
  });
  $effect(() => {
    if (!editor) return;
    void editor.configure({ language, layout: inline ? 'inline' : 'sideBySide', hideUnchanged: hideSame, ignoreWhitespace,
      readOnly: { left: !tickets[0], right: !tickets[1] }, locked: busy }).catch(reason => { error = String(reason); });
    if (active) requestAnimationFrame(() => editor?.layout());
  });
  $effect(() => {
    if (!editor) return;
    app.editorActions[identity] = { save, dirty: dirty.some(Boolean), next, copy, canCopyLeft, canCopyRight,
      canSave: !stale && !busy && dirty.some((value, index) => value && writable[index]), canSaveLeft: !stale && !busy && dirty[0] && writable[0], canSaveRight: !stale && !busy && dirty[1] && writable[1],
      canNavigate: !stale && !busy && !computing && changeCount > 0, canUndo: !stale && !busy && app.capability('recovery').supported && undoIds.length > 0, undo: undoSave,
      saveReasons: readOnlyReasons, undoReason: app.capability('recovery').reason };
  });
</script>

<section class="file-compare">
  <Header {snapshot} path={view.path} count={changeCount ? `${Math.max(1, hunkIndex + 1)} / ${changeCount}` : computing ? 'Computing' : 'Identical'}
    onback={compareFullscreen.active ? () => void compareFullscreen.back() : undefined}
    previous={command('difference-previous')} next={command('difference-next')} saveRight={command('editor-save-right')} onexecute={execute}>
    {#snippet actions()}<MoreMenu fileLeft={command('copy-left')} fileRight={command('copy-right')} undo={command('file-undo')} onexecute={execute}
      remember={extension && editor?.capabilities.highlight !== false ? { extension, on: remembered } : null} onremember={on => rememberLanguage(on ? language : null)} />{/snippet}
  </Header>
  {#if pendingCopy}<ConfirmBar message={copyMessage} confirmLabel={`Copy to ${pendingCopy}`} onconfirm={() => closeCopyBar(true)} oncancel={() => closeCopyBar(false)} />{/if}
  {#if snapshot}<PaneHeads {snapshot} {tickets} {formats} {dirty} reasons={readOnlyReasons} saveLeft={command('editor-save-left')}
    copyLeft={command('hunk-left')} copyRight={command('hunk-right')} oncopy={askCopy} onexecute={execute} />{/if}
  {#if stale}<p class="compare-message warn">Comparison changed. Reopen this file from the folder comparison.</p>{/if}
  {#if notice}<p class="editor-notice" role="status">{notice}</p>{/if}
  {#each [...new Set(readOnlyReasons.filter(Boolean))] as reason}<p class="compare-message" role="status">Read-only: {reason}</p>{/each}
  {#if loading}<p class="compare-message"><span class="spin"></span> Loading...</p>{/if}
  {#if error}<p class="editor-error warn" role="alert">{error}</p>{/if}
  {#if fallback}<p class="compare-message">{fallback}</p>{/if}
  <div class="editor-host" bind:this={host} hidden={!!fallback || stale}></div>
  <Footer {counts} bind:inline bind:hideSame bind:ignoreWhitespace onlanguage={pickLanguage}
    language={{ id: language, label: languageLabel(view.path, language), enabled: editor?.capabilities.highlight !== false }} />
</section>
