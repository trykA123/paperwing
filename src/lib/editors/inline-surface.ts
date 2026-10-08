import { redo, undo } from '@codemirror/commands';
import { getChunks, getOriginalDoc, originalDocChangeEffect, unifiedMergeView } from '@codemirror/merge';
import { ChangeSet } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import type { EditorChange, Side } from '../editor';
import { kindOf, type Band } from './chunk-bands';
import { diffConfigFor } from './merge-diff';
import { markCurrentChange } from './merge-marks';
import { spanOf, type TextEdit } from './line-spans';
import { other } from './shared';
import { LayoutEvents, collapseOptions, dispatchEdit, type Surface, type SurfaceInit } from './surface';

/** The primary side is the editable document; the other side is shown as deleted blocks and can only change through copy. */
export function mountInline(init: SurfaceInit): Surface {
  const { primary } = init, secondary = other(primary);
  let lastEdit: 'primary' | 'secondary' = 'primary';
  const layout = new LayoutEvents();
  const view = new EditorView({
    parent: init.host, doc: init.docs[primary],
    extensions: [init.extensions(primary), EditorView.updateListener.of(update => {
      if (update.docChanged) lastEdit = 'primary';
      if (update.docChanged || update.heightChanged || update.viewportChanged || update.geometryChanged) layout.emit();
    }), unifiedMergeView({
      original: init.docs[secondary], gutter: false, mergeControls: false, highlightChanges: true,
      diffConfig: diffConfigFor(init.ignoreWhitespace), collapseUnchanged: collapseOptions(init.hideUnchanged),
    })],
  });
  const secondaryUndo: ChangeSet[] = [], secondaryRedo: ChangeSet[] = [];
  const chunks = () => getChunks(view.state)?.chunks ?? [];
  const resize = new ResizeObserver(() => layout.emit());
  resize.observe(view.scrollDOM);
  const bands = (): Band[] => {
    const scroller = view.scrollDOM, shift = view.documentTop - scroller.getBoundingClientRect().top + scroller.scrollTop, original = getOriginalDoc(view.state);
    const at = (pos: number) => view.lineBlockAt(Math.min(pos, view.state.doc.length));
    return chunks().map((chunk, index): Band => {
      const top = at(chunk.fromB).top + shift, removed = chunk.fromB === chunk.toB;
      const bottom = removed ? top + spanOf(original, chunk.fromA, chunk.toA).count * view.defaultLineHeight : at(chunk.endB).bottom + shift;
      return { index, kind: kindOf(chunk), top, bottom, wedgeA: false, wedgeB: false };
    });
  };
  const doc = (side: Side) => (side === primary ? view.state.doc : getOriginalDoc(view.state));
  const applySecondary = (changes: ChangeSet) => {
    view.dispatch({ effects: originalDocChangeEffect(view.state, changes) });
    init.onText(secondary);
  };
  const reverse = (from: ChangeSet[], to: ChangeSet[]) => {
    const changes = from.pop()!;
    to.push(changes.invert(getOriginalDoc(view.state)));
    applySecondary(changes);
    return true;
  };
  return {
    views: [view],
    viewOf: () => view,
    scroller: () => view.scrollDOM,
    doc,
    chunks,
    changes: (): EditorChange[] => chunks().map(chunk => {
      const mine = spanOf(view.state.doc, chunk.fromB, chunk.toB), theirs = spanOf(getOriginalDoc(view.state), chunk.fromA, chunk.toA);
      return primary === 'right' ? { left: theirs, right: mine } : { left: mine, right: theirs };
    }),
    edit: (side, edit: TextEdit) => {
      if (side === primary) { dispatchEdit(view, edit); return; }
      const original = getOriginalDoc(view.state);
      const changes = ChangeSet.of(edit, original.length);
      secondaryUndo.push(changes.invert(original)); secondaryRedo.length = 0; lastEdit = 'secondary';
      applySecondary(changes);
    },
    reveal: index => {
      const chunk = chunks()[index];
      if (chunk) view.dispatch({ effects: EditorView.scrollIntoView(Math.min(chunk.fromB, view.state.doc.length), { y: 'center' }) });
    },
    undo: () => {
      if (lastEdit === 'secondary' && secondaryUndo.length) return reverse(secondaryUndo, secondaryRedo);
      return undo(view) || (secondaryUndo.length > 0 && reverse(secondaryUndo, secondaryRedo));
    },
    redo: () => (secondaryRedo.length > 0 && lastEdit === 'secondary' ? reverse(secondaryRedo, secondaryUndo) : redo(view) || (secondaryRedo.length > 0 && reverse(secondaryRedo, secondaryUndo))),
    bands,
    onLayout: listener => layout.add(listener),
    markCurrent: index => {
      const chunk = chunks()[index];
      markCurrentChange(view, chunk && chunk.fromB !== chunk.toB ? { from: chunk.fromB, to: chunk.endB } : null);
    },
    destroy: () => { resize.disconnect(); layout.clear(); view.destroy(); },
  };
}
