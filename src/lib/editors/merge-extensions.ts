import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { bracketMatching, indentOnInput } from '@codemirror/language';
import { highlightSelectionMatches, search, searchKeymap } from '@codemirror/search';
import { Compartment, EditorState, type Extension } from '@codemirror/state';
import { drawSelection, EditorView, highlightActiveLineGutter, highlightSpecialChars, keymap, lineNumbers } from '@codemirror/view';
import { mergeTheme } from './merge-theme';

export const languageSlot = new Compartment();
export const readOnlySlot = new Compartment();
export const darkSlot = new Compartment();

export const readOnlyExtension = (readOnly: boolean): Extension =>
  readOnly ? [EditorState.readOnly.of(true), EditorView.contentAttributes.of({ 'aria-readonly': 'true' })] : [];

export const darkExtension = (dark: boolean): Extension => EditorView.darkTheme.of(dark);

const base: Extension = [
  lineNumbers(), highlightActiveLineGutter(), highlightSpecialChars(), history(), drawSelection(),
  indentOnInput(), bracketMatching(), highlightSelectionMatches(), search({ top: true }), mergeTheme,
  keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap, indentWithTab]),
];

export type ViewSettings = { language: Extension; readOnly: boolean; dark: boolean };

export function viewExtensions(settings: ViewSettings, onUpdate: Extension): Extension {
  return [
    base, onUpdate,
    languageSlot.of(settings.language), readOnlySlot.of(readOnlyExtension(settings.readOnly)), darkSlot.of(darkExtension(settings.dark)),
  ];
}
