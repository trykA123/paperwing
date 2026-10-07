import type { StreamParser } from '@codemirror/language';

type M4State = { depth: number };

const BUILTINS = new Set(['define', 'undefine', 'defn', 'pushdef', 'popdef', 'ifdef', 'ifelse', 'include', 'sinclude', 'divert', 'undivert', 'divnum',
  'changequote', 'changecom', 'changeword', 'eval', 'incr', 'decr', 'len', 'index', 'substr', 'translit', 'patsubst', 'regexp', 'format', 'shift',
  'syscmd', 'esyscmd', 'sysval', 'maketemp', 'mkstemp', 'errprint', 'm4exit', 'm4wrap', 'traceon', 'traceoff', 'dumpdef', 'indir', 'builtin']);

export const m4: StreamParser<M4State> = {
  name: 'm4',
  startState: () => ({ depth: 0 }),
  token(stream, state) {
    if (state.depth > 0) {
      while (!stream.eol()) {
        const next = stream.next();
        if (next === '`') state.depth++;
        else if (next === "'" && --state.depth === 0) break;
      }
      return 'string';
    }
    if (stream.eatSpace()) return null;
    if (stream.match('`')) { state.depth = 1; return 'string'; }
    if (stream.match(/^dnl\b/) || stream.match('#')) { stream.skipToEnd(); return 'comment'; }
    if (stream.match(/^\$(?:\d+|[@*#])/)) return 'atom';
    if (stream.match(/^(?:m4|AC|AM|AS|AX|PKG)_\w+/)) return 'keyword';
    if (stream.match(/^[A-Za-z_]\w*/)) return BUILTINS.has(stream.current()) ? 'keyword' : null;
    if (stream.match(/^\d+/)) return 'number';
    stream.next();
    return null;
  },
};
