import { wordParser } from './rules';

export const batch = wordParser({
  caseless: true,
  words: {
    keyword: ['echo', 'set', 'setlocal', 'endlocal', 'if', 'else', 'not', 'exist', 'defined', 'errorlevel', 'goto', 'call', 'for', 'in', 'do', 'exit', 'pause', 'shift', 'rem',
      'cd', 'chdir', 'copy', 'xcopy', 'robocopy', 'del', 'erase', 'md', 'mkdir', 'rd', 'rmdir', 'move', 'ren', 'rename', 'type', 'start', 'title', 'cls', 'pushd', 'popd', 'choice', 'timeout'],
    atom: ['on', 'off', 'equ', 'neq', 'lss', 'leq', 'gtr', 'geq', 'nul'],
  },
  extra: stream => {
    if (stream.sol()) stream.eatSpace();
    if (stream.match(/^@?rem\b/i) || stream.match('::')) { stream.skipToEnd(); return 'comment'; }
    if (stream.match(/^@/)) return 'punctuation';
    if (/^\s*$/.test(stream.string.slice(0, stream.pos)) && stream.match(/^:[A-Za-z_][\w.-]*/)) return 'typeName';
    if (stream.match(/^%(?:[\w~$:.,=-]+%|\d|\*)|^!\w+!/)) return 'atom';
    return undefined;
  },
});
