import { getChunks } from '@codemirror/merge';
import { RangeSet, RangeSetBuilder, StateEffect, StateField, type Extension } from '@codemirror/state';
import { Decoration, EditorView, GutterMarker, gutter, gutterLineClass, ViewPlugin, type DecorationSet, type ViewUpdate } from '@codemirror/view';
import { kindOf, type ChangeKind } from './chunk-bands';
import { markedLines, type MarkedLine } from './mark-lines';

const GLYPH: Record<ChangeKind, string> = { add: '+', rem: '−', chg: '~' };
const WORD: Record<ChangeKind, string> = { add: 'added line', rem: 'removed line', chg: 'changed line' };

type LineRange = { from: number; to: number };

class CurrentLine extends GutterMarker { elementClass = 'cm-mk-cur'; }
const currentLine = new CurrentLine();

const setCurrent = StateEffect.define<LineRange | null>();
const currentField = StateField.define<RangeSet<GutterMarker>>({
  create: () => RangeSet.empty,
  update(value, transaction) {
    for (const effect of transaction.effects) {
      if (!effect.is(setCurrent)) continue;
      const builder = new RangeSetBuilder<GutterMarker>(), doc = transaction.newDoc;
      if (effect.value) {
        const last = doc.lineAt(Math.min(effect.value.to, doc.length)).number;
        for (let number = doc.lineAt(effect.value.from).number; number <= last; number++) builder.add(doc.line(number).from, doc.line(number).from, currentLine);
      }
      return builder.finish();
    }
    return value.map(transaction.changes);
  },
  provide: field => gutterLineClass.from(field),
});

/** Tints the gutter of the lines of the current change; `null` clears it. */
export function markCurrentChange(view: EditorView, lines: LineRange | null) {
  view.dispatch({ effects: setCurrent.of(lines) });
}

class GlyphMarker extends GutterMarker {
  readonly kind: ChangeKind;
  constructor(kind: ChangeKind) { super(); this.kind = kind; this.elementClass = `cm-mk-gutter cm-mk-${kind}`; }
  eq(other: GlyphMarker) { return other.kind === this.kind; }
  toDOM() {
    const glyph = document.createElement('span');
    glyph.className = 'cm-mk-glyph'; glyph.textContent = GLYPH[this.kind]; glyph.title = WORD[this.kind];
    return glyph;
  }
}

class RemovedBlock extends GutterMarker {
  readonly lines: number;
  constructor(lines: number) { super(); this.lines = lines; this.elementClass = 'cm-mk-gutter cm-mk-rem'; }
  eq(other: RemovedBlock) { return other.lines === this.lines; }
  toDOM() {
    const block = document.createElement('span');
    block.className = 'cm-mk-glyphs';
    for (let line = 0; line < this.lines; line++) block.append(Object.assign(document.createElement('span'), { className: 'cm-mk-glyph', textContent: GLYPH.rem, title: WORD.rem }));
    return block;
  }
}

const glyphs: Record<ChangeKind, GlyphMarker> = { add: new GlyphMarker('add'), rem: new GlyphMarker('rem'), chg: new GlyphMarker('chg') };

const lineClass = (line: MarkedLine) => `cm-mk cm-mk-${line.kind}${line.first ? ' cm-mk-first' : ''}${line.last ? ' cm-mk-last' : ''}`;

class MarkPlugin {
  decorations: DecorationSet = Decoration.none;
  gutterMarkers: RangeSet<GutterMarker> = RangeSet.empty;

  constructor(view: EditorView) { this.build(view); this.toneSpacers(view); }

  update(update: ViewUpdate) {
    const before = getChunks(update.startState)?.chunks, after = getChunks(update.state)?.chunks;
    if (update.docChanged || update.viewportChanged || before !== after) this.build(update.view);
    this.toneSpacers(update.view);
  }

  private build(view: EditorView) {
    const info = getChunks(view.state);
    const lines = info ? markedLines(info.chunks, view.state.doc, info.side === 'a' ? 'a' : 'b', view.viewport) : [];
    const decorations = new RangeSetBuilder<Decoration>(), marks = new RangeSetBuilder<GutterMarker>();
    for (const line of lines) {
      decorations.add(line.from, line.from, Decoration.line({ class: lineClass(line) }));
      marks.add(line.from, line.from, glyphs[line.kind]);
    }
    this.decorations = decorations.finish(); this.gutterMarkers = marks.finish();
  }

  private toneSpacers(view: EditorView) {
    view.requestMeasure({
      key: this,
      read: () => {
        const info = getChunks(view.state);
        if (!info) return [];
        const ends = new Map(info.chunks.map(chunk => [info.side === 'a' ? chunk.toA : chunk.toB, kindOf(chunk)] as const));
        const starts = new Map(info.chunks.map(chunk => [chunk.fromB, kindOf(chunk)] as const));
        const spacers = [...view.dom.querySelectorAll<HTMLElement>('.cm-mergeSpacer')].map(element => [element, ends.get(view.posAtDOM(element)) ?? ''] as const);
        const deleted = [...view.dom.querySelectorAll<HTMLElement>('.cm-deletedChunk')].map(element => [element, starts.get(view.posAtDOM(element)) ?? ''] as const);
        return [...spacers, ...deleted];
      },
      write: spacers => { for (const [element, kind] of spacers) if (element.dataset.mk !== kind) element.dataset.mk = kind; },
    });
  }
}

const plugin = ViewPlugin.fromClass(MarkPlugin, { decorations: value => value.decorations });

const glyphGutter = gutter({
  class: 'cm-markGutter',
  markers: view => view.plugin(plugin)?.gutterMarkers ?? RangeSet.empty,
  initialSpacer: () => glyphs.chg,
  widgetMarker: (view, widget, block) => ('buildDOM' in widget ? new RemovedBlock(Math.max(1, Math.round(block.height / view.defaultLineHeight))) : null),
});

/** Marking E: a bar at the start of each changed line, a +, − or ~ glyph in the gutter, and tone for the alignment gaps. */
export const changeMarks: Extension = [currentField, plugin];
export const changeMarkGutter: Extension = glyphGutter;
