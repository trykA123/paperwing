import { wordParser } from './rules';

const TYPES = ['UBYTE', 'SBYTE', 'UWORD', 'SWORD', 'ULONG', 'SLONG', 'A_UINT64', 'A_INT64', 'FLOAT16_IEEE', 'FLOAT32_IEEE', 'FLOAT64_IEEE'];
const KEYWORDS = ['ASAP2_VERSION', 'A2ML_VERSION', 'ADDR_EPK', 'ECU_ADDRESS', 'ECU_ADDRESS_EXTENSION', 'EPK', 'LOWER_LIMIT', 'UPPER_LIMIT', 'FORMAT', 'PHYS_UNIT',
  'BIT_MASK', 'BYTE_ORDER', 'MSB_FIRST', 'MSB_LAST', 'ADDRESS_TYPE', 'ALIGNMENT_BYTE', 'ALIGNMENT_WORD', 'ALIGNMENT_LONG', 'ANNOTATION_LABEL',
  'ANNOTATION_ORIGIN', 'ANNOTATION_TEXT', 'AXIS_DESCR', 'AXIS_PTS_REF', 'COEFFS', 'COEFFS_LINEAR', 'COMPU_TAB_REF', 'CURVE_AXIS_REF', 'DEPOSIT', 'DISPLAY_IDENTIFIER',
  'EXTENDED_LIMITS', 'FIX_AXIS_PAR', 'FIX_AXIS_PAR_DIST', 'FNC_VALUES', 'FUNCTION_VERSION', 'IN_MEASUREMENT', 'OUT_MEASUREMENT', 'LOC_MEASUREMENT', 'DEF_CHARACTERISTIC',
  'REF_CHARACTERISTIC', 'LAYOUT', 'MAX_REFRESH', 'NUMBER', 'REF_UNIT', 'RESOLUTION', 'ACCURACY', 'STATUS_STRING_REF', 'SYMBOL_LINK', 'DEFAULT_VALUE',
  'DEFAULT_VALUE_NUMERIC', 'NO_AXIS_PTS_X', 'NO_AXIS_PTS_Y', 'AXIS_PTS_X', 'AXIS_PTS_Y', 'SHIFT_OP_X', 'SHIFT_OP_Y', 'SRC_ADDR_X', 'SRC_ADDR_Y', 'VAR_CRITERION',
  'VAR_MEASUREMENT', 'VAR_SELECTOR', 'VAR_SEPARATOR', 'VAR_NAMING', 'VAR_ADDRESS', 'STEP_SIZE', 'SUB_FUNCTION', 'GUARD_RAILS', 'DISCRETE', 'ARRAY_SIZE', 'MATRIX_DIM'];
const ATOMS = ['VALUE', 'CURVE', 'MAP', 'CUBOID', 'CUBE_4', 'CUBE_5', 'VAL_BLK', 'ASCII', 'RAT_FUNC', 'TAB_INTP', 'TAB_NOINTP', 'TAB_VERB', 'LINEAR', 'IDENTICAL',
  'FORM', 'STD_AXIS', 'FIX_AXIS', 'COM_AXIS', 'RES_AXIS', 'CURVE_AXIS', 'READ_ONLY', 'READ_WRITE', 'INDEX_INCR', 'INDEX_DECR', 'DIRECT', 'ON', 'OFF'];

export const asap2 = wordParser({
  line: ['//'], block: ['/*', '*/'],
  words: { typeName: TYPES, keyword: KEYWORDS, atom: ATOMS },
  extra: (stream, state) => {
    if (stream.match(/^\/(?:begin|end)\b/)) { state.mark = stream.current().endsWith('begin') ? 'begin' : null; return 'keyword'; }
    return undefined;
  },
  classify: (_word, state) => {
    if (state.mark !== 'begin') return undefined;
    state.mark = null;
    return 'typeName';
  },
});
