import type { Chunk } from '@codemirror/merge';
import { isolateHistory } from '@codemirror/commands';
import { Transaction, type Extension, type StateEffect, type Text } from '@codemirror/state';
import type { EditorView } from '@codemirror/view';
import type { EditorChange, Side } from '../editor';
import type { Band } from './chunk-bands';
import type { TextEdit } from './line-spans';

export type SurfaceInit = {
  host: HTMLElement;
  docs: Record<Side, Text>;
  extensions: (side: Side) => Extension;
  hideUnchanged: boolean;
  ignoreWhitespace: boolean;
  primary: Side;
  onText: (side: Side) => void;
  onJump: (index: number) => void;
};

export interface Surface {
  readonly views: readonly EditorView[];
  viewOf(side: Side): EditorView;
  scroller(): HTMLElement;
  doc(side: Side): Text;
  chunks(): readonly Chunk[];
  changes(): EditorChange[];
  edit(side: Side, edit: TextEdit): void;
  reveal(index: number): void;
  /** Every change as a vertical band in the pixels of the scroll content. */
  bands(): Band[];
  /** Fires when line positions may have moved: edits, height changes, a new viewport or a resize. */
  onLayout(listener: () => void): () => void;
  markCurrent(index: number): void;
  undo(side: Side): boolean;
  redo(side: Side): boolean;
  destroy(): void;
}

export class LayoutEvents {
  private readonly set = new Set<() => void>();
  add(listener: () => void): () => void { this.set.add(listener); return () => { this.set.delete(listener); }; }
  emit() { for (const listener of [...this.set]) listener(); }
  clear() { this.set.clear(); }
}

export const collapseOptions = (hide: boolean) => (hide ? { margin: 3, minSize: 4 } : undefined);

export function dispatchEdit(view: EditorView, edit: TextEdit, effects: readonly StateEffect<unknown>[] = []) {
  view.dispatch({ changes: edit, effects, annotations: [Transaction.userEvent.of('input.copy-change'), isolateHistory.of('full')] });
}
