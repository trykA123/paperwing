const testModule = 'bun:test';
const { expect, test } = await import(testModule);
import { cleanLanguageMap, extensionOf, findLanguage, languageLabel, LANGUAGES, normalizeExtension, PLAIN_TEXT, resolveLanguage } from './languages';

const BUILT_IN: Record<string, string[]> = {
  cpp: ['c', 'h', 'i', 'cpp', 'cc', 'cxx', 'hpp', 'hh', 'hxx', 'inl'],
  xml: ['xml', 'arxml', 'xsd', 'xsl', 'xslt', 'epc', 'xdm', 'cdd', 'odx', 'odx-d', 'odx-c', 'pdx', 'cdfx', 'fibex', 'vsysvar', 'svd', 'launch', 'cproject', 'project'],
  ini: ['ini', 'cfg', 'conf', 'properties', 'prefs'],
  json: ['json', 'jsonc', 'json5'],
  yaml: ['yml', 'yaml'],
  python: ['py'], shell: ['sh', 'bash'], powershell: ['ps1'], batch: ['bat', 'cmd'], perl: ['pl', 'pm'], lua: ['lua'], sql: ['sql'], csharp: ['cs'], java: ['java'],
  go: ['go'], rust: ['rs'], markdown: ['md'], html: ['html'], css: ['css'], javascript: ['js', 'jsx'], typescript: ['ts', 'tsx'],
  m4: ['m4', 'ac'], asap2: ['a2l', 'aml'], dbc: ['dbc'], ldf: ['ldf'], capl: ['can', 'cin'], oil: ['oil'], linker: ['ld', 'lds', 'lsl', 'lcf', 'icf', 'x'],
  mapfile: ['map', 'lst'], srec: ['s19', 's28', 's37', 'srec', 'mot'], ihex: ['hex', 'ihex'], matlab: ['m'], tlc: ['tlc'], asm: ['s', 'asm', 'inc'],
  makefile: ['mk', 'mak'], cmake: ['cmake'],
};

test('every listed extension resolves to its language, in any case and any folder', () => {
  for (const [id, extensions] of Object.entries(BUILT_IN)) {
    for (const extension of extensions) {
      expect(resolveLanguage(`dir/sub.dir/file.${extension}`)).toBe(id);
      expect(resolveLanguage(`C:\\work\\FILE.${extension.toUpperCase()}`)).toBe(id);
    }
  }
});

test('file names resolve without an extension', () => {
  const names: Record<string, string> = { Makefile: 'makefile', GNUmakefile: 'makefile', 'CMakeLists.txt': 'cmake', SConstruct: 'python', Jenkinsfile: 'java', 'configure.ac': 'm4' };
  for (const [name, id] of Object.entries(names)) expect(resolveLanguage(`repo/${name}`)).toBe(id);
});

test('binary and unknown formats stay plain text', () => {
  for (const path of ['a.slx', 'a.mdl', 'a.elf', 'a.out', 'a.bin', 'notes.txt', 'noextension', 'archive.tar.gz']) expect(resolveLanguage(path)).toBe(PLAIN_TEXT);
});

test('the first line decides when the extension is unknown', () => {
  expect(resolveLanguage('config', {}, '<?xml version="1.0"?>\n<a/>')).toBe('xml');
  expect(resolveLanguage('run', {}, '#!/usr/bin/env bash\necho')).toBe('shell');
  expect(resolveLanguage('run', {}, '#!/bin/sh\n')).toBe('shell');
  expect(resolveLanguage('run', {}, '#!/usr/bin/python3\n')).toBe('python');
  expect(resolveLanguage('run', {}, '#!/usr/bin/perl -w\n')).toBe('perl');
  expect(resolveLanguage('macros', {}, 'dnl header\ndefine(`A\', `1\')')).toBe('m4');
  expect(resolveLanguage('macros', {}, 'divert(-1)\n')).toBe('m4');
  expect(resolveLanguage('plain', {}, 'just text\n')).toBe(PLAIN_TEXT);
  expect(resolveLanguage('a.c', {}, '<?xml version="1.0"?>')).toBe('cpp');
});

test('user mappings override the built-in table and sniffing', () => {
  expect(resolveLanguage('a.c', { c: 'python' })).toBe('python');
  expect(resolveLanguage('a.cfg', { cfg: 'xml' })).toBe('xml');
  expect(resolveLanguage('a.foo', { foo: 'asap2' })).toBe('asap2');
  expect(resolveLanguage('a.foo', { foo: PLAIN_TEXT }, '<?xml')).toBe(PLAIN_TEXT);
  expect(resolveLanguage('a.c', { c: 'missing' })).toBe('cpp');
  expect(resolveLanguage('a.h', { c: 'python' })).toBe('cpp');
});

test('labels name the flavour and the base language', () => {
  expect(languageLabel('a.arxml', 'xml')).toBe('ARXML (XML)');
  expect(languageLabel('a.xml', 'xml')).toBe('XML');
  expect(languageLabel('a.dbc', 'dbc')).toBe('DBC');
  expect(languageLabel('a.zzz', PLAIN_TEXT)).toBe('Plain text');
});

test('extensions are read from the last dot of the file name', () => {
  expect(extensionOf('a.b/file')).toBe('');
  expect(extensionOf('.project')).toBe('project');
  expect(extensionOf('dir\\X.Y.ARXML')).toBe('arxml');
});

test('saved mappings are cleaned and old settings without a map load', () => {
  expect(cleanLanguageMap(undefined)).toEqual({});
  expect(cleanLanguageMap([])).toEqual({});
  expect(cleanLanguageMap({ '.CFG': 'xml', bad: 'nope', 'a b': 'xml', x: 7, tmp: PLAIN_TEXT })).toEqual({ cfg: 'xml', tmp: PLAIN_TEXT });
  expect(normalizeExtension(' .Foo ')).toBe('foo');
  expect(normalizeExtension('a/b')).toBe('');
});

test('the registry has unique ids, extensions and a loader for each language', async () => {
  const extensions = LANGUAGES.flatMap(entry => entry.extensions);
  expect(new Set(extensions).size).toBe(extensions.length);
  expect(new Set(LANGUAGES.map(entry => entry.id)).size).toBe(LANGUAGES.length);
  for (const entry of LANGUAGES) {
    expect(findLanguage(entry.id)).toBe(entry);
    expect(await entry.load()).toBeDefined();
  }
});
