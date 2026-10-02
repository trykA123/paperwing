<script lang="ts">
  import { ask, confirm } from '../lib/confirm';
  import type { editor as MonacoEditor } from 'monaco-editor';
  import { api, type EditFile } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import type { CompareState } from '../lib/compare.svelte';
  import { tabId, type View } from '../lib/workspace';
  import { refLabel } from '../lib/compare-view';
  import { commands, execute } from '../lib/commands';
  import { decodeText, encodeText, copyHunk, type TextFormat } from '../lib/editor';
  import Icon from './Icon.svelte';

  let { view, comparison, active }: { view: Extract<View, { kind: 'fileDiff' }>; comparison: CompareState; active: boolean } = $props();
  let host: HTMLDivElement;
  let editor = $state.raw<MonacoEditor.IStandaloneDiffEditor | null>(null);
  let loading = $state(false), error = $state(''), fallback = $state(''), busy = $state(false);
  let inline = $state(false), hideSame = $state(false), ignoreWhitespace = $state(false);
  let dirty = $state([false, false]), hunkIndex = $state(-1), computing = $state(true);
  let hunks = $state<MonacoEditor.ILineChange[]>([]);
  let formats = $state<TextFormat[]>([]);
  let tickets = $state<(EditFile | null)[]>([]);
  let diskText: string[] = [];
  let undoIds = $state<string[]>([]);
  let ownedTickets = new Set<string>();
  let previousWhitespace: boolean | undefined;
  let reversed = false;
  let diffUnavailable = $state(false);
  let revision = 0;
  const snapshot = $derived(comparison.snapshot);
  const stale = $derived(!snapshot || snapshot.id !== view.sessionId || snapshot.generation !== view.generation);
  const file = $derived(comparison.files.find(file => file.id === view.fileId));
  const identity = $derived(tabId(view, ''));
  const canCopyLeft = $derived(!stale && !busy && !computing && !!tickets[0] && !!file?.right && hunks.length > 0);
  const canCopyRight = $derived(!stale && !busy && !computing && !!tickets[1] && !!file?.left && hunks.length > 0);
  const editorCommands = $derived(commands());
  function command(id: string) { return editorCommands.find(command => command.id === id)!; }
  $effect(() => {
    const tabIdentity = identity;
    app.copyActions[tabIdentity] = {
      left: !stale && !busy && snapshot?.left.endpoint.reference.kind === 'workingTree' && file?.right?.kind === 'file' && !file.right.reason,
      right: !stale && !busy && snapshot?.right.endpoint.reference.kind === 'workingTree' && file?.left?.kind === 'file' && !file.left.reason,
      copy: side => app.requestCopy(view.comparisonId, view.fileId, side),
    };
    return () => { delete app.copyActions[tabIdentity]; };
  });
  function models() {
    const model = editor?.getModel();
    return model && reversed ? { original: model.modified, modified: model.original } : model;
  }
  function control(index: number) { return (index === 0) !== reversed ? editor!.getOriginalEditor() : editor!.getModifiedEditor(); }
  function updateDirty() {
    const model = models();
    dirty = [!!tickets[0] && model?.original.getValue() !== diskText[0], !!tickets[1] && model?.modified.getValue() !== diskText[1]];
    computing = true; hunks = [];
  }

  async function save(side?: number) {
    if (busy || stale || !editor) return;
    busy = true; error = '';
    try {
      const model = models(); if (!model) return;
      for (const index of side === undefined ? [0, 1] : [side]) {
        if (!dirty[index] || !tickets[index]) continue;
        const text = index === 0 ? model.original.getValue() : model.modified.getValue();
        const record = await api.fileSave(tickets[index]!.ticket, encodeText(text, formats[index]));
        diskText[index] = text; undoIds.push(record.id); undoIds = undoIds.slice(-32);
        if (record.warning) app.toast(record.warning, 'warn');
      }
      dirty = [model.original.getValue() !== diskText[0] && !!tickets[0], model.modified.getValue() !== diskText[1] && !!tickets[1]];
      app.toast('Saved', 'success');
    } catch (reason) { error = String(reason); }
    finally {
      const model = models();
      if (model) dirty = [model.original.getValue() !== diskText[0] && !!tickets[0], model.modified.getValue() !== diskText[1] && !!tickets[1]];
      busy = false;
    }
  }

  async function guard() {
    if (busy) return false;
    if (!dirty.some(Boolean)) return true;
    if (await ask(`Save changes to ${view.path}?`, { title: 'Unsaved changes', kind: 'warning', okLabel: 'Save', cancelLabel: 'Other options' })) {
      await save(); return !dirty.some(Boolean);
    }
    if (!await confirm(`Discard unsaved buffer changes to ${view.path}?`, { title: 'Unsaved changes', kind: 'warning', okLabel: 'Discard', cancelLabel: 'Keep editing', destructive: true })) return false;
    const model = models(); model?.original.setValue(diskText[0]); model?.modified.setValue(diskText[1]); dirty = [false, false]; return true;
  }

  function next(direction: number) {
    if (!editor || computing || !hunks.length) return;
    const current = hunkIndex < 0 ? direction > 0 ? -1 : 0 : hunkIndex;
    hunkIndex = (current + direction + hunks.length) % hunks.length;
    const hunk = hunks[hunkIndex];
    control(1).revealLineInCenter(Math.max(1, hunk.modifiedStartLineNumber));
    control(0).revealLineInCenter(Math.max(1, hunk.originalStartLineNumber));
  }

  function copy(side: 'left' | 'right') {
    const index = side === 'left' ? 0 : 1;
    if (index === 0 ? !canCopyLeft : !canCopyRight) return;
    const model = models(); const hunk = hunks[Math.max(0, hunkIndex)]; if (!model || !hunk) return;
    const source = index === 0 ? model.modified : model.original;
    const target = index === 0 ? model.original : model.modified;
    const value = copyHunk(source.getValue(), target.getValue(),
      index === 0 ? hunk.modifiedStartLineNumber : hunk.originalStartLineNumber,
      index === 0 ? hunk.modifiedEndLineNumber : hunk.originalEndLineNumber,
      index === 0 ? hunk.originalStartLineNumber : hunk.modifiedStartLineNumber,
      index === 0 ? hunk.originalEndLineNumber : hunk.modifiedEndLineNumber);
    const targetControl = control(index);
    targetControl.pushUndoStop(); targetControl.executeEdits('copy-hunk', [{ range: target.getFullModelRange(), text: value }]); targetControl.pushUndoStop();
  }

  async function undoSave() {
    if (!undoIds.length || !await guard()) return;
    const id = undoIds.at(-1); if (!id) return;
    busy = true; error = '';
    try {
      await api.recoveryUndo(id); undoIds = undoIds.filter(record => record !== id);
      const model = models();
      for (const index of [0, 1]) {
        if (!tickets[index]) continue;
        await api.editClose(tickets[index]!.ticket);
        ownedTickets.delete(tickets[index]!.ticket);
        tickets[index] = await api.editOpen(view.sessionId, view.generation, view.fileId, index === 0 ? 'left' : 'right');
        ownedTickets.add(tickets[index]!.ticket);
        formats[index] = decodeText(tickets[index]!.bytes); diskText[index] = formats[index].text;
        (index === 0 ? model?.original : model?.modified)?.setValue(diskText[index]);
      }
      dirty = [false, false]; app.toast('Filesystem operation undone', 'success');
    } catch (reason) { error = String(reason); } finally { busy = false; }
  }

  $effect(() => {
    const sessionId = view.sessionId, generation = view.generation, fileId = view.fileId, path = view.path;
    const tabIdentity = identity;
    if (!host || stale || !file) return;
    const current = ++revision; let disposed = false;
    const handles: { dispose: () => void }[] = [];
    const opened = new Set<string>(); ownedTickets = opened;
    const endpoints = [snapshot!.left.endpoint, snapshot!.right.endpoint];
    loading = true; fallback = ''; error = '';
    (async () => {
      const { monaco, language, applyEditorTheme } = await import('../lib/monaco');
      const contents = await Promise.all((['left', 'right'] as const).map(side => file![side] ? api.comparisonContent(sessionId, generation, fileId, side) : null));
      if (disposed || current !== revision) return;
      if (contents.some(content => content && (content.binary || content.kind !== 'file'))) { fallback = 'Binary, linked, or repository content is read-only. Use whole-file copy where supported.'; return; }
      formats = contents.map(content => decodeText(content?.bytes ?? []));
      tickets = [null, null];
      for (const index of [0, 1]) {
        if (endpoints[index].reference.kind !== 'workingTree' || !formats[index].editable) continue;
        const ticket = await api.editOpen(sessionId, generation, fileId, index === 0 ? 'left' : 'right');
        if (disposed || current !== revision) { void api.editClose(ticket.ticket).catch(() => {}); return; }
        opened.add(ticket.ticket); tickets[index] = ticket;
      }
      if (disposed || current !== revision) return;
      formats = formats.map((format, index) => tickets[index] ? decodeText(tickets[index]!.bytes) : format);
      for (const index of [0, 1]) if (tickets[index] && !formats[index].editable) {
        const ticket = tickets[index]!; await api.editClose(ticket.ticket); opened.delete(ticket.ticket); tickets[index] = null;
      }
      if (disposed || current !== revision) return;
      diskText = formats.map(format => format.text);
      const original = monaco.editor.createModel(diskText[0], language(path), monaco.Uri.parse(`paperwing://${sessionId}/${generation}/${fileId}/left/${encodeURIComponent(path)}`));
      const modified = monaco.editor.createModel(diskText[1], language(path), monaco.Uri.parse(`paperwing://${sessionId}/${generation}/${fileId}/right/${encodeURIComponent(path)}`));
      handles.push(original, modified);
      reversed = false;
      const instance = monaco.editor.createDiffEditor(host, { automaticLayout: true, renderSideBySide: !inline, useInlineViewWhenSpaceIsLimited: false,
        readOnly: !tickets[1], originalEditable: !!tickets[0], ignoreTrimWhitespace: ignoreWhitespace,
        hideUnchangedRegions: { enabled: hideSame }, renderOverviewRuler: true, diffAlgorithm: 'advanced', maxComputationTime: 0,
        minimap: { enabled: false }, fontSize: 13, scrollBeyondLastLine: false, renderIndicators: true,
        theme: applyEditorTheme(),
        fontFamily: getComputedStyle(document.documentElement).getPropertyValue('--mono') });
      editor = instance;
      instance.setModel({ original, modified }); handles.push(instance);
      handles.push(original.onDidChangeContent(updateDirty), modified.onDidChangeContent(updateDirty));
      handles.push(instance.onDidUpdateDiff(() => {
        const result = instance.getLineChanges();
        diffUnavailable = result === null;
        const changes = result ?? [];
        hunks = reversed ? changes.map(change => ({ ...change,
          originalStartLineNumber: change.modifiedStartLineNumber, originalEndLineNumber: change.modifiedEndLineNumber,
          modifiedStartLineNumber: change.originalStartLineNumber, modifiedEndLineNumber: change.originalEndLineNumber })) : changes;
        computing = false; hunkIndex = Math.min(hunkIndex, hunks.length - 1);
      }));
      const appearance = new MutationObserver(() => {
        applyEditorTheme();
        editor?.updateOptions({ fontFamily: getComputedStyle(document.documentElement).getPropertyValue('--mono') });
      });
      appearance.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme', 'style'] });
      handles.push({ dispose: () => appearance.disconnect() });
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
    const reverseInline = inline && !!tickets[0] && !tickets[1];
    if (reversed !== reverseInline) {
      const model = models();
      reversed = reverseInline; computing = true; hunks = []; hunkIndex = -1;
      if (model) editor.setModel(reversed ? { original: model.modified, modified: model.original } : model);
    }
    if (previousWhitespace !== undefined && previousWhitespace !== ignoreWhitespace) { computing = true; hunks = []; }
    previousWhitespace = ignoreWhitespace;
    editor.updateOptions({ renderSideBySide: !inline, useInlineViewWhenSpaceIsLimited: false,
      readOnly: busy || !(reversed ? tickets[0] : tickets[1]), originalEditable: !busy && !!(reversed ? tickets[1] : tickets[0]),
      ignoreTrimWhitespace: ignoreWhitespace, hideUnchangedRegions: { enabled: hideSame } });
    if (active) requestAnimationFrame(() => editor?.layout());
  });
  $effect(() => {
    if (!editor) return;
    app.editorActions[identity] = { save, dirty: dirty.some(Boolean), next, copy, canCopyLeft, canCopyRight,
      canSave: !stale && !busy && dirty.some(Boolean), canSaveLeft: !stale && !busy && dirty[0], canSaveRight: !stale && !busy && dirty[1],
      canNavigate: !stale && !busy && !computing && hunks.length > 0, canUndo: !stale && !busy && undoIds.length > 0, undo: undoSave };
  });
</script>

<section class="file-compare">
  <header class="compare-summary editor-toolbar"><strong class="mono">{view.path}</strong><span class="grow"></span>
    <button class="btn small icon-only flip" title="Previous difference (Shift+F7)" aria-label="Previous difference" disabled={!command('difference-previous').enabled} onclick={() => execute(command('difference-previous'))}><Icon name="chevron" /></button>
    <span class="editor-count">{hunks.length ? `${Math.max(1, hunkIndex + 1)} / ${hunks.length}` : computing ? 'Computing' : diffUnavailable ? 'Unavailable' : 'Identical'}</span>
    <button class="btn small icon-only" title="Next difference (F7)" aria-label="Next difference" disabled={!command('difference-next').enabled} onclick={() => execute(command('difference-next'))}><Icon name="chevron" /></button>
    <div class="seg small"><button class:on={!inline} onclick={() => inline = false}>Side by side</button><button class:on={inline} onclick={() => inline = true}>Inline</button></div>
    <label class="check"><input type="checkbox" bind:checked={hideSame} /> Hide unchanged</label>
    <label class="check"><input type="checkbox" bind:checked={ignoreWhitespace} /> Ignore whitespace</label>
  </header>
  {#if snapshot}<div class="editor-endpoints">{#each [snapshot.left.endpoint, snapshot.right.endpoint] as endpoint, index}
    <div><strong>{index === 0 ? 'Left' : 'Right'} @ {refLabel(endpoint.reference)}</strong><span class="grow"></span>
      <span class="faint">{tickets[index] ? formats[index]?.eol === '\r\n' ? 'UTF-8 · CRLF' : formats[index]?.eol === '\r' ? 'UTF-8 · CR' : 'UTF-8 · LF' : 'Read-only'}{dirty[index] ? ' · Unsaved' : ''}</span>
      <button class="btn" disabled={!command(index === 0 ? 'editor-save-left' : 'editor-save-right').enabled} onclick={() => execute(command(index === 0 ? 'editor-save-left' : 'editor-save-right'))}><Icon name="check" /> Save</button>
    </div>{/each}</div>{/if}
  {#if stale}<p class="compare-message warn">Comparison changed. Reopen this file from the folder comparison.</p>{/if}
  {#if loading}<p class="compare-message"><span class="spin"></span> Loading...</p>{/if}
  {#if error}<p class="editor-error warn" role="alert">{error}</p>{/if}
  {#if fallback}<p class="compare-message">{fallback}</p>{/if}
  <div class="monaco-host" bind:this={host} hidden={!!fallback || stale}></div>
  <footer class="editor-footer"><button class="btn" title="Copy selected hunk to left (Ctrl+Alt+Left)" disabled={!command('hunk-left').enabled} onclick={() => execute(command('hunk-left'))}><Icon name="copy" /> To left</button>
    <button class="btn" title="Copy selected hunk to right (Ctrl+Alt+Right)" disabled={!command('hunk-right').enabled} onclick={() => execute(command('hunk-right'))}><Icon name="copy" /> To right</button>
    <button class="btn" disabled={!command('copy-left').enabled} onclick={() => execute(command('copy-left'))}><Icon name="copy" /> File to left</button>
    <button class="btn" disabled={!command('copy-right').enabled} onclick={() => execute(command('copy-right'))}><Icon name="copy" /> File to right</button>
    <span class="grow"></span><button class="btn" disabled={!command('file-undo').enabled} onclick={() => execute(command('file-undo'))}><Icon name="refresh" /> Undo saved operation</button></footer>
</section>