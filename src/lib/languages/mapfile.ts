import type { StreamParser } from '@codemirror/language';

export const mapfile: StreamParser<null> = {
  name: 'mapfile',
  startState: () => null,
  token(stream) {
    if (stream.sol() && stream.match(/^[A-Z][A-Za-z ]+$/)) return 'keyword';
    if (stream.eatSpace()) return null;
    if (stream.match(/^0[xX][\da-fA-F]+/)) return 'number';
    if (stream.sol() && stream.match(/^[\da-fA-F]{8,16}\b/)) return 'number';
    if (stream.match(/^\*fill\*|^\*\([^)]*\)|^\*\w*/)) return 'comment';
    if (stream.match(/^\.[\w.$-]+/)) return 'propertyName';
    if (stream.match(/^[\w./\\:+-]*\.(?:o|obj|a|lib|elf|out)\b(?:\([^)]*\))?/)) return 'string';
    if (stream.match(/^\d+\b/)) return 'number';
    if (stream.match(/^[A-Za-z_$][\w$]*/)) return null;
    stream.next();
    return null;
  },
};
