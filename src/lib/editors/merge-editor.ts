import type { Chunk } from '@codemirror/merge';
import { openSearchPanel } from '@codemirror/search';
import { Text, type Extension, type StateEffect } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { currentTheme, type CompareEditor, type CompareEditorInit, type EditorChange, type EditorEvent, type EditorSettings, type Side } from '../editor';
import { findLanguage } from '../languages';
import { contentBytes, type SideContent, type TextFormat } from '../text-format';
import { mountInline } from './inline-surface';
import { linesOf, replaceSpan } from './line-spans';
import { darkExtension, darkSlot, languageSlot, readOnlyExtension, readOnlySlot, viewExtensions } from './merge-extensions';
import { mountSideBySide } from './side-by-side-surface';
import { Listeners, SIDES, stepIndex } from './shared';
import type { Surface } from './surface';

const toText = (text: string) => Text.of(text.split('\n'));

async function loadLanguage(id: string): Promise<Extension> {
  try { return (await findLanguage(id)?.load()) ?? []; }
  catch (reason) { console.warn(`Could not load the ${id} language`, reason); return []; }
}

class MergeEditor implements CompareEditor {
  readonly kind = 'merge' as const;
  readonly capabilities = { edit: true, search: true, highlight: true };
  private contents: Record<Side, SideContent>;
  private baseline: Record<Side, Text>;
  private settings: EditorSettings;
  private language: Extension;
  private surface: Surface;
  private current = -1;
  private lastEdited: Side = 'right';
  private cache: { chunks: readonly Chunk[]; list: EditorChange[] } | null = null;
  private disposed = false;
  private readonly listeners = new Listeners();
  private readonly appearance: MutationObserver;
  private readonly host: HTMLElement;

  constructor(init: CompareEditorInit, language: Extension) {
    this.host = init.host;
    this.contents = { left: init.left, right: init.right };
    this.baseline = { left: toText(init.left.format.text), right: toText(init.right.format.text) };
    this.settings = init.settings;
    this.language = language;
    this.surface = this.mount({ left: this.baseline.left, right: this.baseline.right });
    this.appearance = new MutationObserver(() => { void this.configure({ theme: currentTheme() }); this.layout(); });
    this.appearance.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme', 'style'] });
  }

  private primarySide(): Side {
    const fixed = (side: Side) => this.settings.readOnly[side] || !this.contents[side].format.editable;
    return this.settings.layout === 'inline' && fixed('right') && !fixed('left') ? 'left' : 'right';
  }

  private mount(docs: Record<Side, Text>): Surface {
    const watch = (side: Side) => EditorView.updateListener.of(update => { if (update.docChanged) { this.lastEdited = side; this.textChanged(side); } });
    const mount = this.settings.layout === 'inline' ? mountInline : mountSideBySide;
    return mount({
      host: this.host, docs, hideUnchanged: this.settings.hideUnchanged, ignoreWhitespace: this.settings.ignoreWhitespace, primary: this.primarySide(),
      extensions: side => viewExtensions({ language: this.language, readOnly: this.isReadOnly(side), dark: this.settings.theme === 'dark' }, watch(side)),
      onText: side => this.textChanged(side),
    });
  }

  private replaceSurface(docs: Record<Side, Text>) {
    this.surface.destroy();
    this.surface = this.mount(docs);
    this.changed();
  }

  private remount() {
    const docs = { left: this.surface.doc('left'), right: this.surface.doc('right') };
    const top = this.surface.scroller().scrollTop, focused = this.host.contains(document.activeElement);
    this.replaceSurface(docs);
    const restore = () => { this.surface.scroller().scrollTop = top; };
    restore(); requestAnimationFrame(restore);
    if (focused) this.focus();
  }

  private textChanged(side: Side) {
    this.listeners.emit({ type: 'text', side });
    this.changed();
  }

  private changed() {
    this.current = Math.min(this.current, this.changes().length - 1);
    this.listeners.emit({ type: 'changes' });
  }

  private reconfigure(effect: (side: Side) => StateEffect<unknown>) {
    for (const side of SIDES) this.surface.viewOf(side).dispatch({ effects: effect(side) });
  }

  async configure(patch: Partial<EditorSettings>): Promise<void> {
    const next: EditorSettings = { ...this.settings, ...patch, readOnly: { ...this.settings.readOnly, ...patch.readOnly } };
    const language = patch.language !== undefined && patch.language !== this.settings.language ? await loadLanguage(patch.language) : null;
    if (this.disposed) return;
    const previous = this.settings, previousPrimary = this.primarySide();
    this.settings = next;
    if (language) this.language = language;
    const structural = next.layout !== previous.layout || next.hideUnchanged !== previous.hideUnchanged || next.ignoreWhitespace !== previous.ignoreWhitespace
      || (next.layout === 'inline' && this.primarySide() !== previousPrimary);
    if (structural) { this.remount(); return; }
    if (language) this.reconfigure(() => languageSlot.reconfigure(language));
    if (next.theme !== previous.theme) this.reconfigure(() => darkSlot.reconfigure(darkExtension(next.theme === 'dark')));
    if (next.locked !== previous.locked || SIDES.some(side => next.readOnly[side] !== previous.readOnly[side])) this.reconfigure(side => readOnlySlot.reconfigure(readOnlyExtension(this.isReadOnly(side))));
  }

  setContent(side: Side, content: SideContent) {
    this.contents[side] = content;
    this.baseline[side] = toText(content.format.text);
    this.revert(side);
  }

  getText(side: Side): string { return this.surface.doc(side).toString(); }
  getBytes(side: Side): number[] { return contentBytes(this.contents[side], this.getText(side)); }
  format(side: Side): TextFormat { return this.contents[side].format; }
  isReadOnly(side: Side): boolean { return this.settings.locked || this.settings.readOnly[side] || !this.contents[side].format.editable; }
  isDirty(side: Side): boolean { return !this.surface.doc(side).eq(this.baseline[side]); }
  markSaved(side: Side) { this.baseline[side] = this.surface.doc(side); }

  revert(side: Side) {
    const docs = { left: this.surface.doc('left'), right: this.surface.doc('right') };
    docs[side] = this.baseline[side];
    this.replaceSurface(docs);
  }

  changes(): readonly EditorChange[] {
    const chunks = this.surface.chunks();
    if (this.cache?.chunks !== chunks) this.cache = { chunks, list: this.surface.changes() };
    return this.cache.list;
  }

  currentChange(): number { return this.current; }

  goToChange(direction: 1 | -1): number {
    this.current = stepIndex(this.current, direction, this.changes().length);
    this.surface.reveal(this.current);
    return this.current;
  }

  copyChange(from: Side, to: Side, index = Math.max(0, this.current)): boolean {
    const change = this.changes()[index];
    if (!change || from === to || this.isReadOnly(to)) return false;
    this.surface.edit(to, replaceSpan(this.surface.doc(to), change[to], linesOf(this.surface.doc(from), change[from])));
    return true;
  }

  undo(): boolean { return this.surface.undo(this.lastEdited); }
  redo(): boolean { return this.surface.redo(this.lastEdited); }
  openSearch(): boolean { return openSearchPanel(this.surface.viewOf(this.lastEdited)); }
  focus() { this.surface.viewOf(this.lastEdited).focus(); }
  layout() { for (const view of this.surface.views) view.requestMeasure(); }
  on(listener: (event: EditorEvent) => void) { return this.listeners.add(listener); }

  dispose() {
    this.disposed = true;
    this.appearance.disconnect();
    this.surface.destroy();
    this.listeners.clear();
  }
}

export async function createMergeEditor(init: CompareEditorInit): Promise<CompareEditor> {
  return new MergeEditor(init, await loadLanguage(init.settings.language));
}
