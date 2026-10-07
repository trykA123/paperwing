const testModule = 'bun:test';
const { expect, test } = await import(testModule);
import { StringStream, type StreamParser } from '@codemirror/language';
import { asap2 } from './asap2';
import { asm } from './asm';
import { batch } from './batch';
import { capl } from './capl';
import { dbc } from './dbc';
import { ihex } from './ihex';
import { ldf } from './ldf';
import { linker } from './linker';
import { m4 } from './m4';
import { makefile } from './makefile';
import { mapfile } from './mapfile';
import { oil } from './oil';
import { srec } from './srec';
import { tlc } from './tlc';

type Token = [string, string];

function tokenize(parser: StreamParser<any>, source: string): Token[][] {
  const state = parser.startState!(2);
  return source.split('\n').map(line => {
    const stream = new StringStream(line, 4, 2), tokens: Token[] = [];
    let end = -1;
    if (!line) { parser.blankLine?.(state, 2); return tokens; }
    while (!stream.eol()) {
      stream.start = stream.pos;
      const style = parser.token(stream, state);
      if (stream.pos <= stream.start) throw new Error(`Parser stalled at ${stream.pos} in "${line}"`);
      if (!style) continue;
      const last = tokens.at(-1);
      if (last?.[1] === style && end === stream.start) last[0] += stream.current();
      else tokens.push([stream.current(), style]);
      end = stream.pos;
    }
    return tokens;
  });
}

const styles = (parser: StreamParser<any>, source: string, ...wanted: string[]) =>
  tokenize(parser, source).flat().filter(([, style]) => wanted.includes(style)).map(([text, style]) => `${style}:${text}`);

test('m4 colours dnl comments, quoted strings, builtins and arguments', () => {
  const [comment, define, nested] = tokenize(m4, "dnl a comment\ndefine(`NAME', `$1 and $@')\nifelse(`a `b'', 1)");
  expect(comment).toEqual([['dnl a comment', 'comment']]);
  expect(define).toContainEqual(['define', 'keyword']);
  expect(define).toContainEqual(["`NAME'", 'string']);
  expect(tokenize(m4, 'echo $1 $@')[0]).toContainEqual(['$1', 'atom']);
  expect(nested).toContainEqual(["`a `b''", 'string']);
});

test('m4 quotes span lines and autoconf macros are keywords', () => {
  const lines = tokenize(m4, "AC_DEFUN([X], `one\ntwo')\nAC_INIT");
  expect(lines[1]).toEqual([["two'", 'string']]);
  expect(lines[0]).toContainEqual(['AC_DEFUN', 'keyword']);
});

test('A2L colours blocks, block names, types, strings and numbers', () => {
  const source = '/begin MEASUREMENT speed "Vehicle speed" UWORD NO_COMPU_METHOD 0 0 0.0 255.5\n  ECU_ADDRESS 0x1000 /* c */\n/end MEASUREMENT // done';
  const tokens = tokenize(asap2, source);
  expect(tokens[0]).toContainEqual(['/begin', 'keyword']);
  expect(tokens[0]).toContainEqual(['MEASUREMENT', 'typeName']);
  expect(tokens[0]).toContainEqual(['"Vehicle speed"', 'string']);
  expect(tokens[0]).toContainEqual(['UWORD', 'typeName']);
  expect(tokens[0]).toContainEqual(['255.5', 'number']);
  expect(tokens[1]).toContainEqual(['ECU_ADDRESS', 'keyword']);
  expect(tokens[1]).toContainEqual(['0x1000', 'number']);
  expect(tokens[1]).toContainEqual(['/* c */', 'comment']);
  expect(tokens[2]).toContainEqual(['/end', 'keyword']);
  expect(tokens[2]).toContainEqual(['// done', 'comment']);
});

test('DBC colours keywords, names, bit layouts and strings', () => {
  const tokens = tokenize(dbc, 'BO_ 500 EngineData: 8 ECU\n SG_ Speed : 0|16@1+ (0.1,0) [0|250] "km/h" Gateway\nCM_ SG_ 500 Speed "The speed";');
  expect(tokens[0]).toContainEqual(['BO_', 'keyword']);
  expect(tokens[0]).toContainEqual(['500', 'number']);
  expect(tokens[0]).toContainEqual(['EngineData', 'typeName']);
  expect(tokens[1]).toContainEqual(['SG_', 'keyword']);
  expect(tokens[1]).toContainEqual(['Speed', 'propertyName']);
  expect(tokens[1]).toContainEqual(['0|16@1+', 'atom']);
  expect(tokens[1]).toContainEqual(['"km/h"', 'string']);
  expect(tokens[2]).toContainEqual(['CM_', 'keyword']);
});

test('LDF colours header keywords, block names and comments', () => {
  const tokens = tokenize(ldf, 'LIN_protocol_version = "2.1";\nNodes {\n  Master: ECU, 5 ms, 0.1 ms;\n}\nFrames { // frames\n}');
  expect(tokens[0]).toContainEqual(['LIN_protocol_version', 'keyword']);
  expect(tokens[0]).toContainEqual(['"2.1"', 'string']);
  expect(tokens[1]).toContainEqual(['Nodes', 'keyword']);
  expect(tokens[2]).toContainEqual(['5', 'number']);
  expect(tokens[4]).toContainEqual(['// frames', 'comment']);
});

test('CAPL colours event handlers, types and calls', () => {
  const tokens = tokenize(capl, 'variables { msTimer t; }\non message EngineData {\n  write("rpm %d", this.RPM);\n  setTimer(t, 100);\n}');
  expect(tokens[0]).toContainEqual(['variables', 'keyword']);
  expect(tokens[0]).toContainEqual(['msTimer', 'typeName']);
  expect(tokens[1]).toContainEqual(['on', 'keyword']);
  expect(tokens[1]).toContainEqual(['message', 'typeName']);
  expect(tokens[2]).toContainEqual(['write', 'keyword']);
  expect(tokens[2]).toContainEqual(['"rpm %d"', 'string']);
  expect(tokens[3]).toContainEqual(['setTimer', 'keyword']);
  expect(tokens[3]).toContainEqual(['100', 'number']);
});

test('OIL colours object kinds, attributes and values', () => {
  const tokens = tokenize(oil, 'OIL_VERSION = "2.5";\nTASK Main {\n  PRIORITY = 3;\n  AUTOSTART = TRUE { APPMODE = Std; };\n};');
  expect(tokens[0]).toContainEqual(['OIL_VERSION', 'propertyName']);
  expect(tokens[1]).toContainEqual(['TASK', 'keyword']);
  expect(tokens[2]).toContainEqual(['PRIORITY', 'propertyName']);
  expect(tokens[2]).toContainEqual(['3', 'number']);
  expect(tokens[3]).toContainEqual(['TRUE', 'atom']);
  expect(tokens[3]).toContainEqual(['APPMODE', 'propertyName']);
});

test('linker scripts colour commands, section names, the location counter and sizes', () => {
  const tokens = tokenize(linker, 'MEMORY { FLASH (rx) : ORIGIN = 0x08000000, LENGTH = 512K }\nSECTIONS {\n  .text : { *(.text*) . = ALIGN(4); } > FLASH\n}');
  expect(tokens[0]).toContainEqual(['MEMORY', 'keyword']);
  expect(tokens[0]).toContainEqual(['ORIGIN', 'keyword']);
  expect(tokens[0]).toContainEqual(['0x08000000', 'number']);
  expect(tokens[0]).toContainEqual(['512K', 'number']);
  expect(tokens[2]).toContainEqual(['.text', 'propertyName']);
  expect(tokens[2]).toContainEqual(['.', 'atom']);
  expect(tokens[2]).toContainEqual(['ALIGN', 'keyword']);
});

test('map files colour headings, addresses, sections and object files', () => {
  const tokens = tokenize(mapfile, 'Memory Configuration\n .text           0x0000000008000000      0x1c4 build/main.o\n                0x0000000008000010                main');
  expect(tokens[0]).toEqual([['Memory Configuration', 'keyword']]);
  expect(tokens[1]).toContainEqual(['.text', 'propertyName']);
  expect(tokens[1]).toContainEqual(['0x0000000008000000', 'number']);
  expect(tokens[1]).toContainEqual(['build/main.o', 'string']);
  expect(tokens[2]).toContainEqual(['0x0000000008000010', 'number']);
});

test('S-records split into type, count, address, data and checksum', () => {
  const [header, data, last, broken] = tokenize(srec, 'S00600004844521B\nS1137AF00A0A0D0000000000000000000000000061\nS9030000FC\nS1ZZ');
  expect(header.map(([, style]) => style)).toEqual(['keyword', 'number', 'propertyName', 'atom']);
  expect(data).toEqual([['S1', 'keyword'], ['13', 'number'], ['7AF0', 'propertyName'], ['61', 'atom']]);
  expect(last.map(([text]) => text)).toEqual(['S9', '03', '0000', 'FC']);
  expect(broken).toEqual([['S1ZZ', 'invalid']]);
});

test('Intel HEX splits into mark, count, address, type, data and checksum', () => {
  const [data, eof, broken] = tokenize(ihex, ':10010000214601360121470136007EFE09D2190140\n:00000001FF\n:12');
  expect(data.map(([, style]) => style)).toEqual(['punctuation', 'number', 'propertyName', 'keyword', 'atom']);
  expect(data.map(([text]) => text)).toEqual([':', '10', '0100', '00', '40']);
  expect(eof.map(([text]) => text)).toEqual([':', '00', '0000', '01', 'FF']);
  expect(broken).toEqual([[':12', 'invalid']]);
});

test('TLC colours directives, comments and variables', () => {
  const tokens = tokenize(tlc, '%% a comment\n%if EXISTS($Name)\n  %assign x = "text" + 3\n%endif');
  expect(tokens[0]).toEqual([['%% a comment', 'comment']]);
  expect(tokens[1]).toContainEqual(['%if', 'keyword']);
  expect(tokens[1]).toContainEqual(['$Name', 'atom']);
  expect(tokens[2]).toContainEqual(['%assign', 'keyword']);
  expect(tokens[2]).toContainEqual(['"text"', 'string']);
  expect(tokens[3]).toContainEqual(['%endif', 'keyword']);
});

test('assembler colours labels, directives, mnemonics, registers and comments', () => {
  const tokens = tokenize(asm, 'start:\n  .global main\n  mov r0, #0x10 ; load\n  addi sp, sp, -16 // frame\n* old comment');
  expect(tokens[0]).toContainEqual(['start', 'typeName']);
  expect(tokens[1]).toContainEqual(['.global', 'keyword']);
  expect(tokens[2]).toContainEqual(['mov', 'keyword']);
  expect(tokens[2]).toContainEqual(['r0', 'atom']);
  expect(tokens[2]).toContainEqual(['0x10', 'number']);
  expect(tokens[2]).toContainEqual(['; load', 'comment']);
  expect(tokens[3]).toContainEqual(['sp', 'atom']);
  expect(tokens[4]).toEqual([['* old comment', 'comment']]);
});

test('Makefiles colour directives, targets, assignments, variables and comments', () => {
  const tokens = tokenize(makefile, '# build\nCC := gcc\nifeq ($(OS),Linux)\nendif\nall: main.o\n\t$(CC) -o $@ $< # link');
  expect(tokens[0]).toEqual([['# build', 'comment']]);
  expect(tokens[1]).toContainEqual(['CC', 'propertyName']);
  expect(tokens[2]).toContainEqual(['ifeq', 'keyword']);
  expect(tokens[2]).toContainEqual(['OS', 'atom']);
  expect(tokens[4]).toContainEqual(['all', 'typeName']);
  expect(tokens[5]).toContainEqual(['$@', 'atom']);
  expect(tokens[5]).toContainEqual(['# link', 'comment']);
});

test('batch files colour comments, labels, commands and variables', () => {
  const tokens = tokenize(batch, 'REM start\n:: also a comment\n:loop\n@echo off\nset NAME=%1\nif exist "%NAME%" goto loop');
  expect(tokens[0]).toEqual([['REM start', 'comment']]);
  expect(tokens[1]).toEqual([[':: also a comment', 'comment']]);
  expect(tokens[2]).toContainEqual([':loop', 'typeName']);
  expect(tokens[3]).toContainEqual(['echo', 'keyword']);
  expect(tokens[3]).toContainEqual(['off', 'atom']);
  expect(tokens[4]).toContainEqual(['%1', 'atom']);
  expect(tokens[5]).toContainEqual(['goto', 'keyword']);
  expect(styles(batch, 'echo %PATH%', 'atom')).toEqual(['atom:%PATH%']);
});

test('every grammar keeps tokenizing an empty and a long line', () => {
  const long = 'x '.repeat(5000);
  for (const parser of [m4, asap2, dbc, ldf, capl, oil, linker, mapfile, srec, ihex, tlc, asm, makefile, batch]) {
    expect(() => tokenize(parser, `\n${long}\n`)).not.toThrow();
  }
});
