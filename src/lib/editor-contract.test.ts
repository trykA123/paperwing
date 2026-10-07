const testModule = 'bun:test';
const { afterAll, beforeAll, describe, expect, test } = await import(testModule);
import { GlobalRegistrator } from '@happy-dom/global-registrator';
import { createCompareEditor, type CompareEditor, type EditorSettings, type EngineKind } from './editor';
import { readContent } from './text-format';

const ENGINES: { kind: EngineKind; editable: boolean }[] = [{ kind: 'merge', editable: true }, { kind: 'viewer', editable: false }];
const settings = (overrides: Partial<EditorSettings> = {}): EditorSettings => ({
  layout: 'sideBySide', theme: 'light', language: 'plaintext', hideUnchanged: false, ignoreWhitespace: false,
  readOnly: { left: false, right: false }, locked: false, ...overrides,
});
const encode = (text: string) => Array.from(new TextEncoder().encode(text));
const FILES = {
  lf: 'one\ntwo\nthree\n', crlf: 'one\r\ntwo\r\nthree\r\n', cr: 'one\rtwo\rthree\r', bom: '\uFEFFone\ntwo\nthree\n', doubleBom: '\uFEFF\uFEFFone\ntwo\nthree\n', bomCrlf: '\uFEFFone\r\ntwo\r\nthree\r\n',
  noFinalNewline: 'one\ntwo\nthree', noFinalNewlineCrlf: 'one\r\ntwo\r\nthree', empty: '', single: 'only line',
};
const MIXED = { mixed: 'one\r\ntwo\nthree\r\n', mixedBom: '\uFEFFone\ntwo\r\nthree\n' };
const hosts: HTMLElement[] = [];
const editors: CompareEditor[] = [];

async function open(kind: EngineKind, left: string, right: string, overrides: Partial<EditorSettings> = {}): Promise<CompareEditor> {
  const host = document.createElement('div');
  document.body.append(host); hosts.push(host);
  const editor = await createCompareEditor({ host, kind, left: readContent(encode(left)), right: readContent(encode(right)), settings: settings(overrides) });
  editors.push(editor);
  return editor;
}

beforeAll(() => { GlobalRegistrator.register({ width: 1200, height: 800 }); });
afterAll(() => {
  for (const editor of editors) editor.dispose();
  for (const host of hosts) host.remove();
  return GlobalRegistrator.unregister();
});

for (const { kind, editable } of ENGINES) {
  describe(`editor contract: ${kind}`, () => {
    for (const [name, text] of Object.entries({ ...FILES, ...MIXED })) {
      test(`${name} round-trips byte for byte when nothing is edited`, async () => {
        const editor = await open(kind, text, text);
        expect(editor.getBytes('left')).toEqual(encode(text));
        expect(editor.getBytes('right')).toEqual(encode(text));
        expect(editor.isDirty('left')).toBe(false);
      });
    }

    test('mixed line endings are never editable and keep their bytes', async () => {
      const editor = await open(kind, MIXED.mixed, MIXED.mixed.replace('two', 'TWO'));
      expect(editor.format('left').editable).toBe(false);
      expect(editor.isReadOnly('left')).toBe(true);
      expect(editor.copyChange('right', 'left')).toBe(false);
      expect(editor.getBytes('left')).toEqual(encode(MIXED.mixed));
    });

    test('changes are listed in document order with whole-line spans', async () => {
      const left = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'].join('\n'), right = ['a', 'B', 'c', 'd', 'e', 'f', 'x', 'g', 'h'].join('\n');
      const editor = await open(kind, left, right);
      expect(editor.changes()).toEqual([
        { left: { start: 1, count: 1 }, right: { start: 1, count: 1 } },
        { left: { start: 6, count: 0 }, right: { start: 6, count: 1 } },
      ]);
    });

    test('next and previous walk the changes in order and wrap', async () => {
      const left = 'a\nb\nc\nd\ne\nf\ng', right = 'a\nB\nc\nD\ne\nF\ng';
      const editor = await open(kind, left, right);
      expect(editor.currentChange()).toBe(-1);
      expect([1, 1, 1, 1].map(() => editor.goToChange(1))).toEqual([0, 1, 2, 0]);
      expect([-1, -1].map(() => editor.goToChange(-1))).toEqual([2, 1]);
      const fresh = await open(kind, left, right);
      expect(fresh.goToChange(-1)).toBe(2);
    });

    test('whitespace-only differences can be ignored', async () => {
      const editor = await open(kind, 'a\n  b\nc', 'a\nb  \nc', { ignoreWhitespace: true });
      expect(editor.changes()).toEqual([]);
      await editor.configure({ ignoreWhitespace: false });
      expect(editor.changes()).toHaveLength(1);
    });

    test('inline layout reports the same changes', async () => {
      const editor = await open(kind, 'a\nb\nc\n', 'a\nB\nc\nd\n', { layout: 'inline' });
      expect(editor.changes()).toEqual([{ left: { start: 1, count: 1 }, right: { start: 1, count: 1 } }, { left: { start: 3, count: 0 }, right: { start: 3, count: 1 } }]);
    });

    if (!editable) {
      test('the read-only renderer refuses every edit', async () => {
        const editor = await open(kind, 'a\nb', 'a\nB');
        expect(editor.copyChange('right', 'left')).toBe(false);
        expect(editor.undo()).toBe(false);
        expect(editor.isReadOnly('left') && editor.isReadOnly('right')).toBe(true);
        expect(editor.isDirty('left') || editor.isDirty('right')).toBe(false);
        expect(editor.getText('left')).toBe('a\nb');
      });
      return;
    }

    for (const [name, text] of Object.entries(FILES).filter(([key]) => key !== 'empty' && key !== 'single')) {
      test(`${name} keeps its BOM, line endings and final newline when one line is edited`, async () => {
        const editor = await open(kind, text, text.replace('two', 'TWO edited'));
        expect(editor.copyChange('right', 'left', 0)).toBe(true);
        expect(editor.isDirty('left')).toBe(true);
        expect(editor.getBytes('left')).toEqual(encode(text.replace('two', 'TWO edited')));
        expect(editor.changes()).toEqual([]);
      });
    }

    const copies: [string, string, string, string][] = [
      ['replacement', 'one\nchanged\nthree', 'one\nold\nthree', 'one\nchanged\nthree'],
      ['insertion', 'one\ninserted\nthree', 'one\nthree', 'one\ninserted\nthree'],
      ['deletion', 'one\nthree', 'one\nremoved\nthree', 'one\nthree'],
      ['insertion at the end', 'one\ntwo\nthree', 'one\ntwo', 'one\ntwo\nthree'],
      ['deletion at the end', 'one\ntwo', 'one\ntwo\nextra', 'one\ntwo'],
      ['deletion of the first line', 'two', 'one\ntwo', 'two'],
      ['replacement of everything', 'x\ny', 'a\nb\nc', 'x\ny'],
    ];
    for (const [name, source, target, result] of copies) {
      test(`copying a ${name} hunk to the other side gives the source lines`, async () => {
        const editor = await open(kind, target, source);
        expect(editor.copyChange('right', 'left', 0)).toBe(true);
        expect(editor.getText('left')).toBe(result);
        expect(editor.getText('right')).toBe(source);
        expect(editor.changes()).toEqual([]);
      });
      test(`copying a ${name} hunk the other way works in inline layout`, async () => {
        const editor = await open(kind, source, target, { layout: 'inline' });
        expect(editor.copyChange('left', 'right', 0)).toBe(true);
        expect(editor.getText('right')).toBe(result);
        expect(editor.changes()).toEqual([]);
      });
    }

    test('a hunk copy is one undo step and can be redone', async () => {
      const editor = await open(kind, 'a\nB\nc', 'a\nb\nc');
      editor.copyChange('left', 'right', 0);
      expect(editor.getText('right')).toBe('a\nB\nc');
      expect(editor.undo()).toBe(true);
      expect(editor.getText('right')).toBe('a\nb\nc');
      expect(editor.redo()).toBe(true);
      expect(editor.getText('right')).toBe('a\nB\nc');
    });

    test('a hunk copy keeps a doubled BOM and one text change only', async () => {
      const text = FILES.doubleBom;
      const editor = await open(kind, text, text.replace('two', 'TWO'));
      expect(editor.copyChange('right', 'left', 0)).toBe(true);
      expect(editor.getBytes('left')).toEqual(encode(text.replace('two', 'TWO')));
      expect(editor.getBytes('left').slice(0, 6)).toEqual([239, 187, 191, 239, 187, 191]);
    });

    test('an edit made after the snapshot stays unsaved when the snapshot is marked saved', async () => {
      const editor = await open(kind, 'a\nb\nc\nd\ne', 'a\nX\nc\nY\ne');
      editor.copyChange('right', 'left', 0);
      const snapshot = editor.snapshot('left');
      expect(snapshot.bytes).toEqual(encode('a\nX\nc\nd\ne'));
      editor.copyChange('right', 'left', 0);
      editor.markSaved('left', snapshot);
      expect(editor.isDirty('left')).toBe(true);
      expect(editor.getText('left')).toBe('a\nX\nc\nY\ne');
      editor.markSaved('left');
      expect(editor.isDirty('left')).toBe(false);
    });

    test('inline layout keeps a writable left side writable after the editor is locked and unlocked', async () => {
      const editor = await open(kind, 'a\nb', 'a\nB', { layout: 'inline', readOnly: { left: false, right: true } });
      const content = hosts.at(-1)!.querySelector('.cm-content')!;
      const enter = () => content.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', keyCode: 13, bubbles: true, cancelable: true }));
      await editor.configure({ locked: true });
      expect(content.getAttribute('aria-readonly')).toBe('true');
      enter();
      expect(editor.getText('left')).toBe('a\nb');
      await editor.configure({ locked: false });
      expect(editor.isReadOnly('left')).toBe(false);
      expect(content.getAttribute('aria-readonly')).toBeNull();
      enter();
      expect(editor.getText('left')).not.toBe('a\nb');
      expect(editor.isDirty('left')).toBe(true);
    });

    test('copying into a read-only side is refused', async () => {
      const editor = await open(kind, 'a\nB', 'a\nb', { readOnly: { left: false, right: true } });
      expect(editor.copyChange('left', 'right', 0)).toBe(false);
      expect(editor.getText('right')).toBe('a\nb');
    });

    test('saved text becomes the clean baseline and revert restores it', async () => {
      const editor = await open(kind, 'a\nB', 'a\nb');
      editor.copyChange('left', 'right', 0);
      expect(editor.isDirty('right')).toBe(true);
      editor.markSaved('right');
      expect(editor.isDirty('right')).toBe(false);
      editor.copyChange('right', 'left', 0);
      editor.revert('left');
      expect(editor.getText('left')).toBe('a\nB');
      expect(editor.isDirty('left')).toBe(false);
    });

    test('text and change events fire for a copy', async () => {
      const editor = await open(kind, 'a\nB', 'a\nb');
      const seen: string[] = [];
      editor.on(event => seen.push(event.type === 'text' ? `text:${event.side}` : event.type));
      editor.copyChange('left', 'right', 0);
      expect(seen).toContain('text:right');
      expect(seen).toContain('changes');
    });
  });
}
