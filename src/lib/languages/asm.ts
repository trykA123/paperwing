import type { StreamParser } from '@codemirror/language';

type AsmState = { seen: boolean; block: boolean };

const DIRECTIVES = /^(?:db|dw|dd|dq|ds|equ|org|end|ends|section|segment|macro|endm|include|global|globl|extern|public|align|byte|word|long|quad|space|skip|ascii|asciz|string|set|if|else|endif|ifdef|ifndef|define|proc|endp|assume|title|type|size|weak|rept|endr)$/i;
const REGISTER = /^(?:[rxwvasdq]\d{1,2}|sp|lr|pc|fp|ip|psw|sr|cr|ccr|acc|[re]?[abcd]x|[re]?[sd]i|[re]?[sb]p|[abcd][lh]|zero|ra|gp|tp|[ast]\d)$/i;

export const asm: StreamParser<AsmState> = {
  name: 'asm',
  startState: () => ({ seen: false, block: false }),
  token(stream, state) {
    if (stream.sol()) state.seen = false;
    if (state.block) {
      while (!stream.eol()) if (stream.match('*/')) { state.block = false; break; } else stream.next();
      return 'comment';
    }
    if (stream.eatSpace()) return null;
    if (stream.match('/*')) { state.block = true; return 'comment'; }
    if (stream.match(';') || stream.match('//')) { stream.skipToEnd(); return 'comment'; }
    if (stream.sol() && stream.match('*')) { stream.skipToEnd(); return 'comment'; }
    const next = stream.peek() ?? '';
    if (next === '"' || next === "'") {
      stream.next();
      while (!stream.eol() && stream.next() !== next);
      return 'string';
    }
    if (stream.match(/^(?:0[xX][\da-fA-F]+|\$[\da-fA-F]+|[\da-fA-F]+[hH]\b|0[bB][01]+|\d+)/)) return 'number';
    if (!state.seen && stream.match(/^[A-Za-z_.$@?][\w.$@?]*(?=\s*:)/)) { state.seen = false; return 'typeName'; }
    if (stream.match(/^\.[A-Za-z_]\w*/)) { state.seen = true; return 'keyword'; }
    if (stream.match(/^[A-Za-z_.$@?][\w.$@?]*/)) {
      const word = stream.current();
      if (REGISTER.test(word)) return 'atom';
      if (DIRECTIVES.test(word) || !state.seen) { state.seen = true; return 'keyword'; }
      return null;
    }
    stream.next();
    return null;
  },
};
