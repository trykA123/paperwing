import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import type { Extension } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { tags } from '@lezer/highlight';

const changedText = 'color-mix(in oklch, var(--tone) 30%, transparent) !important';
const gapStripe = 'repeating-linear-gradient(135deg, transparent 0 6px, color-mix(in oklch, var(--tone) 28%, transparent) 6px 7px)';
const gapBar = 'repeating-linear-gradient(180deg, var(--tone) 0 4px, transparent 4px 7px)';
const bar = { content: '""', position: 'absolute', left: '0', top: '0', bottom: '0', width: '3px', background: 'var(--tone)' };

const theme = EditorView.theme({
  '&': { backgroundColor: 'var(--editor)', color: 'var(--text)', fontSize: '13px' },
  '&.cm-focused': { outline: '2px solid var(--focus)', outlineOffset: '-2px' },
  '.cm-scroller': { fontFamily: 'var(--mono)', lineHeight: '20px' },
  '.cm-content': { caretColor: 'var(--acc)' },
  '&.cm-focused .cm-cursor': { borderLeftColor: 'var(--acc)' },
  '.cm-gutters': { backgroundColor: 'var(--editor)', color: 'var(--dim)', border: 'none', borderRight: '1px solid var(--line)' },
  '.cm-activeLineGutter': { backgroundColor: 'transparent', color: 'var(--text)' },
  '.cm-selectionBackground': { backgroundColor: 'var(--soft)' },
  '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground': { backgroundColor: 'var(--accs)' },
  '.cm-selectionMatch': { backgroundColor: 'var(--hl-soft)' },
  '&.cm-focused .cm-matchingBracket': { backgroundColor: 'var(--soft)', outline: '1px solid var(--line-ctl)' },
  '&.cm-merge-a .cm-changedLine, &.cm-merge-b .cm-changedLine, .cm-inlineChangedLine, .cm-deletedChunk': { backgroundColor: 'transparent' },
  '.cm-mk-add': { '--tone': 'var(--ok)', '--tt': 'var(--ok-text)' },
  '.cm-mk-rem, .cm-deletedChunk': { '--tone': 'var(--err)', '--tt': 'var(--err-text)' },
  '.cm-mk-chg, .cm-deletedChunk[data-mk="chg"]': { '--tone': 'var(--warn)', '--tt': 'var(--warn-text)' },
  '.cm-deletedChunk[data-mk="rem"] .cm-deletedText': { background: 'none !important' },
  '.cm-line.cm-mk, .cm-deletedChunk': { position: 'relative' },
  '.cm-line.cm-mk::before, .cm-deletedChunk::before': bar,
  '.cm-mk-first::before': { borderTopRightRadius: '3px' },
  '.cm-mk-last::before': { borderBottomRightRadius: '3px' },
  '.cm-changedText, .cm-deletedChunk .cm-deletedText': { background: changedText },
  '.cm-mk-add .cm-changedText, .cm-mk-rem .cm-changedText': { background: 'none !important' },
  '.cm-changedText, .cm-changedText *, .cm-deletedText, .cm-deletedText *': { color: 'var(--text)' },
  '.cm-mk-glyphs': { display: 'block' },
  '.cm-mk-glyph': { display: 'block', height: '20px', lineHeight: '20px' },
  '.cm-markGutter .cm-gutterElement': { width: '16px', padding: '0', textAlign: 'center', fontWeight: '700', color: 'var(--tt)' },
  '.cm-gutterElement.cm-mk-cur': { backgroundColor: 'var(--accs)' },
  '.cm-mergeSpacer': { position: 'relative', '--tone': 'var(--line-ctl)', backgroundImage: gapStripe },
  '.cm-mergeSpacer::before': { ...bar, background: gapBar },
  '.cm-mergeSpacer[data-mk="add"]': { '--tone': 'var(--ok)' },
  '.cm-mergeSpacer[data-mk="rem"]': { '--tone': 'var(--err)' },
  '.cm-mergeSpacer[data-mk="chg"]': { '--tone': 'var(--warn)' },
  '.cm-collapsedLines': { background: 'var(--soft)', color: 'var(--dim)', fontFamily: 'var(--font)', fontSize: '12px', padding: '2px 10px' },
  '.cm-panels': { backgroundColor: 'var(--panel)', color: 'var(--text)', borderColor: 'var(--line)', fontFamily: 'var(--font)' },
  '.cm-search label': { fontSize: '12px' },
  '.cm-textfield': { backgroundColor: 'var(--editor)', color: 'var(--text)', border: '1px solid var(--line-ctl)', borderRadius: '4px' },
  '.cm-button': { backgroundImage: 'none', backgroundColor: 'var(--panel)', color: 'var(--text)', border: '1px solid var(--line-ctl)', borderRadius: '4px' },
  '.cm-searchMatch': { backgroundColor: 'var(--hl-soft)', outline: '1px solid var(--hl)' },
  '.cm-searchMatch-selected': { backgroundColor: 'var(--accs)', outline: '1px solid var(--acc)' },
});

const highlight = HighlightStyle.define([
  { tag: [tags.keyword, tags.controlKeyword, tags.moduleKeyword, tags.operatorKeyword], color: 'var(--acc)' },
  { tag: [tags.string, tags.special(tags.string), tags.regexp], color: 'var(--ok-text)' },
  { tag: [tags.number, tags.bool, tags.null, tags.atom], color: 'var(--warn-text)' },
  { tag: [tags.comment, tags.docComment], color: 'var(--faint-text)', fontStyle: 'italic' },
  { tag: [tags.typeName, tags.className, tags.tagName, tags.namespace], color: 'var(--warn-text)' },
  { tag: [tags.attributeName, tags.propertyName], color: 'var(--ok-text)' },
  { tag: [tags.punctuation, tags.operator, tags.bracket], color: 'var(--dim)' },
  { tag: tags.invalid, color: 'var(--err-text)' },
]);

export const mergeTheme: Extension = [theme, syntaxHighlighting(highlight)];
