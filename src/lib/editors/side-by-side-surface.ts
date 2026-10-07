import { redo, undo } from '@codemirror/commands';
import { MergeView } from '@codemirror/merge';
import { EditorView } from '@codemirror/view';
import type { EditorChange, Side } from '../editor';
import { diffConfigFor } from './merge-diff';
import { spanOf, type TextEdit } from './line-spans';
import { collapseOptions, dispatchEdit, type Surface, type SurfaceInit } from './surface';

export function mountSideBySide(init: SurfaceInit): Surface {
  const merge = new MergeView({
    a: { doc: init.docs.left, extensions: init.extensions('left') }, b: { doc: init.docs.right, extensions: init.extensions('right') },
    parent: init.host, gutter: true, highlightChanges: true, diffConfig: diffConfigFor(init.ignoreWhitespace), collapseUnchanged: collapseOptions(init.hideUnchanged),
  });
  const viewOf = (side: Side): EditorView => (side === 'left' ? merge.a : merge.b);
  return {
    views: [merge.a, merge.b],
    viewOf,
    scroller: () => merge.dom,
    doc: side => viewOf(side).state.doc,
    chunks: () => merge.chunks,
    changes: (): EditorChange[] => merge.chunks.map(chunk => ({
      left: spanOf(merge.a.state.doc, chunk.fromA, chunk.toA), right: spanOf(merge.b.state.doc, chunk.fromB, chunk.toB),
    })),
    edit: (side: Side, edit: TextEdit) => dispatchEdit(viewOf(side), edit),
    reveal: index => {
      const chunk = merge.chunks[index];
      if (chunk) merge.b.dispatch({ effects: EditorView.scrollIntoView(Math.min(chunk.fromB, merge.b.state.doc.length), { y: 'center' }) });
    },
    undo: side => undo(viewOf(side)),
    redo: side => redo(viewOf(side)),
    destroy: () => merge.destroy(),
  };
}
