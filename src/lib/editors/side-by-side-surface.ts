import { redo, undo } from '@codemirror/commands';
import { MergeView } from '@codemirror/merge';
import { EditorView } from '@codemirror/view';
import type { EditorChange, Side } from '../editor';
import { bandsIn, chunkBands, type Band, type LineMetrics } from './chunk-bands';
import { diffConfigFor } from './merge-diff';
import { markCurrentChange } from './merge-marks';
import { spanOf, type TextEdit } from './line-spans';
import { RibbonGutter } from './ribbon-gutter';
import { LayoutEvents, collapseOptions, dispatchEdit, type Surface, type SurfaceInit } from './surface';

const RIBBON_PAD = 4;

function metricsOf(view: EditorView, origin: number): LineMetrics {
  const offset = () => view.documentTop - origin, clamp = (pos: number) => Math.min(pos, view.state.doc.length);
  return { top: pos => view.lineBlockAt(clamp(pos)).top + offset(), bottom: pos => view.lineBlockAt(clamp(pos)).bottom + offset() };
}

export function mountSideBySide(init: SurfaceInit): Surface {
  const layout = new LayoutEvents();
  const watch = EditorView.updateListener.of(update => { if (update.docChanged || update.heightChanged || update.viewportChanged || update.geometryChanged) layout.emit(); });
  const merge = new MergeView({
    a: { doc: init.docs.left, extensions: [init.extensions('left'), watch] }, b: { doc: init.docs.right, extensions: [init.extensions('right'), watch] },
    parent: init.host, gutter: false, highlightChanges: true, diffConfig: diffConfigFor(init.ignoreWhitespace), collapseUnchanged: collapseOptions(init.hideUnchanged),
  });
  const viewOf = (side: Side): EditorView => (side === 'left' ? merge.a : merge.b);
  const origin = () => merge.dom.getBoundingClientRect().top - merge.dom.scrollTop;
  const metrics = () => { const top = origin(); return [metricsOf(merge.a, top), metricsOf(merge.b, top)] as const; };
  let current = -1;
  const ribbons = new RibbonGutter({
    bands: () => { const [a, b] = metrics(); return bandsIn(merge.chunks, merge.a.viewport, RIBBON_PAD, a, b); },
    current: () => current, total: () => merge.chunks.length, onJump: init.onJump,
  });
  merge.b.dom.parentElement!.before(ribbons.element);
  const stopLayout = layout.add(() => ribbons.schedule());
  const resize = new ResizeObserver(() => layout.emit());
  resize.observe(merge.dom);
  ribbons.schedule();
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
    bands: (): Band[] => { const [a, b] = metrics(); return chunkBands(merge.chunks, a, b); },
    onLayout: listener => layout.add(listener),
    markCurrent: index => {
      current = index;
      const chunk = merge.chunks[index];
      markCurrentChange(merge.a, chunk && chunk.fromA !== chunk.toA ? { from: chunk.fromA, to: chunk.endA } : null);
      markCurrentChange(merge.b, chunk && chunk.fromB !== chunk.toB ? { from: chunk.fromB, to: chunk.endB } : null);
      ribbons.schedule();
    },
    undo: side => undo(viewOf(side)),
    redo: side => redo(viewOf(side)),
    destroy: () => { stopLayout(); resize.disconnect(); ribbons.destroy(); layout.clear(); merge.destroy(); },
  };
}
