import type { StreamParser } from '@codemirror/language';

type MakeState = { quote: string | null };

const DIRECTIVES = /^(?:-?include|sinclude|ifeq|ifneq|ifdef|ifndef|else|endif|define|endef|export|unexport|override|vpath|private|undefine)\b/;
const FUNCTIONS = /^(?:subst|patsubst|strip|findstring|filter|filter-out|sort|word|wordlist|words|firstword|lastword|dir|notdir|suffix|basename|addsuffix|addprefix|join|wildcard|realpath|abspath|if|or|and|foreach|call|value|eval|origin|flavor|shell|error|warning|info)\b/;

export const makefile: StreamParser<MakeState> = {
  name: 'makefile',
  startState: () => ({ quote: null }),
  token(stream) {
    if (stream.sol() && !stream.match(/^\t/, false)) {
      if (stream.match(DIRECTIVES)) return 'keyword';
      if (stream.match(/^[\w.${}()-]+(?=\s*(?:[:+?!]?=|::=))/)) return 'propertyName';
      if (stream.match(/^[^\s:=#$][^:=#]*(?=:(?!=))/)) return 'typeName';
    }
    if (stream.eatSpace()) return null;
    if (stream.match('#')) { stream.skipToEnd(); return 'comment'; }
    if (stream.match(/^\$[@<^?*%+|]/)) return 'atom';
    if (stream.match(/^\$[({]/)) return 'bracket';
    if (/\$[({]$/.test(stream.string.slice(0, stream.pos)) && stream.match(FUNCTIONS)) return 'keyword';
    if (stream.match(/^[A-Za-z_][\w.-]*(?=[)}])/)) return 'atom';
    const next = stream.peek() ?? '';
    if (next === '"' || next === "'") {
      stream.next();
      while (!stream.eol() && stream.next() !== next);
      return 'string';
    }
    stream.next();
    return null;
  },
};
