import { wordParser } from './rules';

export const linker = wordParser({
  line: ['//'], block: ['/*', '*/'],
  number: /^(?:0[xX][\da-fA-F]+|\d+)[KMkm]?\b/,
  word: /^[A-Za-z_$][\w$]*/,
  words: {
    keyword: ['SECTIONS', 'MEMORY', 'ENTRY', 'OUTPUT_FORMAT', 'OUTPUT_ARCH', 'OUTPUT', 'INCLUDE', 'INPUT', 'GROUP', 'STARTUP', 'SEARCH_DIR', 'PHDRS', 'VERSION', 'PROVIDE',
      'PROVIDE_HIDDEN', 'HIDDEN', 'KEEP', 'ALIGN', 'ALIGNOF', 'AT', 'FILL', 'SORT', 'SORT_BY_NAME', 'SORT_BY_ALIGNMENT', 'ORIGIN', 'LENGTH', 'ASSERT', 'NOLOAD', 'NOCROSSREFS',
      'EXCLUDE_FILE', 'ADDR', 'LOADADDR', 'SIZEOF', 'SIZEOF_HEADERS', 'DEFINED', 'BYTE', 'SHORT', 'LONG', 'QUAD', 'SQUAD', 'CONSTANT', 'MAX', 'MIN', 'REGION_ALIAS', 'TARGET', 'FLAGS',
      'ONLY_IF_RO', 'ONLY_IF_RW', 'SUBALIGN', 'OVERLAY', 'COMMON', 'CREATE_OBJECT_SYMBOLS', 'CONSTRUCTORS', 'COPY', 'INFO', 'DSECT', 'HLL', 'PT_LOAD', 'PT_NOTE', 'PT_TLS'],
  },
  extra: stream => {
    if (stream.sol() && stream.match(/^#\s*\w+/)) return 'keyword';
    if (stream.match(/^\.[A-Za-z_][\w.*$-]*/)) return 'propertyName';
    if (stream.match(/^\.(?![\w.])/)) return 'atom';
    return undefined;
  },
});
