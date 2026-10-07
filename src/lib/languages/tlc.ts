import { wordParser } from './rules';

export const tlc = wordParser({
  block: ['/*', '*/'],
  words: { atom: ['TRUE', 'FALSE', 'FEVAL', 'LibGetFormattedBlockPath'] },
  extra: stream => {
    if (stream.match('%%') || stream.match('%/')) { stream.skipToEnd(); return 'comment'; }
    if (stream.match(/^%[a-z]+/)) return 'keyword';
    if (stream.match(/^\$\w+/)) return 'atom';
    return undefined;
  },
});
