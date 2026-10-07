const testModule = 'bun:test';
const { expect, test } = await import(testModule);
import { engineFor, LARGE_FILE_BYTES } from './editor';
import { findLanguage, languageId, LANGUAGES } from './languages';
import { contentBytes, contentFromText, readContent } from './text-format';

const encode = (text: string) => Array.from(new TextEncoder().encode(text));

test('files above the limit on either side use the read-only viewer', () => {
  expect(engineFor([LARGE_FILE_BYTES, 0])).toBe('merge');
  expect(engineFor([10, LARGE_FILE_BYTES + 1])).toBe('viewer');
  expect(engineFor([LARGE_FILE_BYTES + 1, LARGE_FILE_BYTES + 1])).toBe('viewer');
  expect(LARGE_FILE_BYTES).toBe(5 * 1024 * 1024);
});

test('the language registry keeps the extension mapping the Monaco view used', () => {
  const expected: Record<string, string> = { c: 'cpp', h: 'cpp', cpp: 'cpp', hpp: 'cpp', rs: 'rust', py: 'python', js: 'javascript', jsx: 'javascript',
    ts: 'typescript', tsx: 'typescript', xml: 'xml', arxml: 'xml', yml: 'yaml', yaml: 'yaml' };
  for (const [extension, id] of Object.entries(expected)) expect(languageId(`dir/File.${extension.toUpperCase()}`)).toBe(id);
  for (const path of ['README.md', 'data.json', 'Makefile', 'archive.tar.gz']) expect(languageId(path)).toBe('plaintext');
  expect(LANGUAGES.every(entry => findLanguage(entry.id) === entry)).toBe(true);
});

test('mixed line endings keep their original bytes and refuse changed text', () => {
  const bytes = encode('﻿one\r\ntwo\nthree');
  const content = readContent(bytes);
  expect(content.format.editable).toBe(false);
  expect(contentBytes(content, content.format.text)).toEqual(bytes);
  expect(() => contentBytes(content, 'changed')).toThrow();
});

test('single-style files restore their BOM and line endings from normalised text', () => {
  for (const text of ['a\r\nb\r\n', '﻿a\nb', 'a\rb\r', '']) {
    const content = readContent(encode(text));
    expect(content.original).toBeNull();
    expect(contentBytes(content, content.format.text)).toEqual(encode(text));
  }
  expect(contentFromText('a\r\nb\rc').format.text).toBe('a\nb\nc');
});
