import { StreamLanguage, type StreamParser } from '@codemirror/language';

export type Cursor = Parameters<StreamParser<unknown>['token']>[0];
export type RuleState = { block: boolean; mark: string | null; line: number };

export type Rules = {
  caseless?: boolean;
  line?: readonly string[];
  block?: readonly [string, string];
  quotes?: string;
  number?: RegExp;
  word?: RegExp;
  words: Readonly<Record<string, readonly string[]>>;
  extra?: (stream: Cursor, state: RuleState) => string | null | undefined;
  classify?: (word: string, state: RuleState, stream: Cursor) => string | null | undefined;
};

const NUMBER = /^(?:0[xX][\da-fA-F]+|\d+\.?\d*(?:[eE][+-]?\d+)?)[uUlLfF]*/;
const WORD = /^[A-Za-z_][\w]*/;

function lookup(words: Rules['words'], caseless: boolean) {
  const table = new Map<string, string>();
  for (const [style, list] of Object.entries(words)) for (const word of list) table.set(caseless ? word.toLowerCase() : word, style);
  return table;
}

function eatString(stream: Cursor, quote: string): string {
  let escaped = false, next: string | void;
  while ((next = stream.next()) != null) {
    if (next === quote && !escaped) break;
    escaped = !escaped && next === '\\';
  }
  return 'string';
}

function eatBlock(stream: Cursor, state: RuleState, close: string): string {
  while (!stream.eol()) {
    if (stream.match(close)) { state.block = false; break; }
    stream.next();
  }
  return 'comment';
}

export function wordParser(rules: Rules): StreamParser<RuleState> {
  const table = lookup(rules.words, !!rules.caseless);
  const number = rules.number ?? NUMBER, wordPattern = rules.word ?? WORD, quotes = rules.quotes ?? '"';
  return {
    name: 'rules',
    startState: () => ({ block: false, mark: null, line: 0 }),
    token(stream, state) {
      if (stream.sol()) state.line++;
      if (state.block && rules.block) return eatBlock(stream, state, rules.block[1]);
      if (stream.eatSpace()) return null;
      for (const prefix of rules.line ?? []) if (stream.match(prefix)) { stream.skipToEnd(); return 'comment'; }
      if (rules.block && stream.match(rules.block[0])) { state.block = true; return eatBlock(stream, state, rules.block[1]); }
      const custom = rules.extra?.(stream, state);
      if (custom !== undefined) return custom;
      const next = stream.peek() ?? '';
      if (quotes.includes(next)) { stream.next(); return eatString(stream, next); }
      if (stream.match(number)) return 'number';
      if (stream.match(wordPattern)) {
        const word = stream.current();
        const own = rules.classify?.(word, state, stream);
        if (own !== undefined) return own;
        return table.get(rules.caseless ? word.toLowerCase() : word) ?? null;
      }
      stream.next();
      return /[{}()[\]]/.test(next) ? 'bracket' : null;
    },
  };
}

export const define = (parser: StreamParser<any>) => StreamLanguage.define(parser);
