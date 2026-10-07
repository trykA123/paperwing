import { wordParser } from './rules';

const KEYWORDS = ['VERSION', 'NS_', 'BS_', 'BU_', 'BO_', 'SG_', 'EV_', 'VAL_', 'VAL_TABLE_', 'CM_', 'BA_', 'BA_DEF_', 'BA_DEF_DEF_', 'BA_DEF_REL_', 'BA_DEF_DEF_REL_', 'BA_REL_',
  'BO_TX_BU_', 'SG_MUL_VAL_', 'SIG_VALTYPE_', 'SIG_GROUP_', 'SIGTYPE_VALTYPE_', 'ENVVAR_DATA_', 'SGTYPE_', 'SGTYPE_VAL_', 'BA_DEF_SGTYPE_', 'BA_SGTYPE_', 'SIG_TYPE_REF_',
  'CAT_DEF_', 'CAT_', 'FILTER', 'EV_DATA_', 'NS_DESC_', 'CM_SG_', 'CRC_', 'BO_TX_BU'];
const NAMES: Record<string, string> = { BO_: 'typeName', SG_: 'propertyName', BU_: 'typeName', EV_: 'propertyName' };

export const dbc = wordParser({
  line: ['//'],
  words: { keyword: KEYWORDS, atom: ['INT', 'FLOAT', 'STRING', 'ENUM', 'HEX', 'Vector__XXX'] },
  extra: stream => (stream.match(/^\d+\|\d+@[01][+-]/) ? 'atom' : undefined),
  classify: (word, state) => {
    if (word in NAMES && KEYWORDS.includes(word)) { state.mark = NAMES[word]!; return 'keyword'; }
    if (!state.mark) return undefined;
    const style = state.mark;
    state.mark = null;
    return style;
  },
});
