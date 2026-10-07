import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import type { Extension } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { tags } from '@lezer/highlight';

// !important: @codemirror/merge sets its own text and gutter colours with higher specificity.
const removedText = 'color-mix(in oklch, var(--err) 26%, transparent) !important';
const addedText = 'color-mix(in oklch, var(--ok) 30%, transparent) !important';

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
  '.cm-changedLine, .cm-insertedLine, .cm-inlineChangedLine': { backgroundColor: 'var(--ok-bg)' },
  '&.cm-merge-a .cm-changedLine, .cm-deletedChunk, .cm-deletedLine': { backgroundColor: 'var(--err-bg)' },
  '.cm-changedText, .cm-insertedText': { background: addedText },
  '&.cm-merge-a .cm-changedText, .cm-deletedChunk .cm-deletedText': { background: removedText },
  '.cm-changedText, .cm-changedText *, .cm-deletedText, .cm-deletedText *, .cm-insertedText, .cm-insertedText *': { color: 'var(--text)' },
  '.cm-changedLineGutter, .cm-insertedLineGutter, .cm-inlineChangedLineGutter': { background: 'var(--ok) !important' },
  '&.cm-merge-a .cm-changedLineGutter, .cm-deletedLineGutter': { background: 'var(--err) !important' },
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
