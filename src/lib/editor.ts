import { utf8Length, type SideContent, type TextFormat } from './text-format';

export const LARGE_FILE_BYTES = 5 * 1024 * 1024;
export const LARGE_FILE_NOTICE = 'Large file: read-only view';

export type Side = 'left' | 'right';
export type EditorLayout = 'sideBySide' | 'inline';
export type EditorTheme = 'light' | 'dark';
export type EngineKind = 'merge' | 'viewer';

/** `start` is the zero-based first line. A `count` of 0 is an insertion point before line `start`. */
export type LineSpan = { start: number; count: number };
export type EditorChange = { left: LineSpan; right: LineSpan };
export type EditorEvent = { type: 'text'; side: Side } | { type: 'changes' };

export type EditorSettings = {
  layout: EditorLayout;
  theme: EditorTheme;
  language: string;
  hideUnchanged: boolean;
  ignoreWhitespace: boolean;
  readOnly: Record<Side, boolean>;
  locked: boolean;
};

/** Bytes to send to the write path plus an opaque token for the exact document they came from. */
export type SideSnapshot = { readonly bytes: number[]; readonly token: unknown };
export type EditorCapabilities = { edit: boolean; search: boolean; highlight: boolean };

export interface CompareEditor {
  readonly kind: EngineKind;
  readonly capabilities: EditorCapabilities;
  configure(patch: Partial<EditorSettings>): Promise<void>;
  setContent(side: Side, content: SideContent): void;
  getText(side: Side): string;
  getBytes(side: Side): number[];
  format(side: Side): TextFormat;
  isReadOnly(side: Side): boolean;
  isDirty(side: Side): boolean;
  snapshot(side: Side): SideSnapshot;
  markSaved(side: Side, snapshot?: SideSnapshot): void;
  revert(side: Side): void;
  changes(): readonly EditorChange[];
  currentChange(): number;
  goToChange(direction: 1 | -1): number;
  copyChange(from: Side, to: Side, index?: number): boolean;
  undo(): boolean;
  redo(): boolean;
  openSearch(): boolean;
  focus(): void;
  layout(): void;
  on(listener: (event: EditorEvent) => void): () => void;
  dispose(): void;
}

export type CompareEditorInit = {
  host: HTMLElement;
  kind: EngineKind;
  left: SideContent;
  right: SideContent;
  settings: EditorSettings;
};

export function engineFor(sizes: readonly number[]): EngineKind {
  return sizes.some(size => size > LARGE_FILE_BYTES) ? 'viewer' : 'merge';
}

export function engineForText(...texts: readonly string[]): EngineKind {
  return engineFor(texts.map(utf8Length));
}

export async function createCompareEditor(init: CompareEditorInit): Promise<CompareEditor> {
  if (init.kind === 'viewer') return (await import('./editors/viewer-editor')).createViewerEditor(init);
  return (await import('./editors/merge-editor')).createMergeEditor(init);
}

export async function loadEngine(kind: EngineKind): Promise<void> {
  await (kind === 'viewer' ? import('./editors/viewer-editor') : import('./editors/merge-editor'));
}

export function currentTheme(): EditorTheme {
  return document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light';
}
