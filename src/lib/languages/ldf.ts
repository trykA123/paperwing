import { wordParser } from './rules';

const KEYWORDS = ['LIN_description_file', 'LIN_protocol_version', 'LIN_language_version', 'LIN_speed', 'Channel_name', 'Nodes', 'Node_attributes', 'Signals', 'Diagnostic_signals',
  'Frames', 'Diagnostic_frames', 'Event_triggered_frames', 'Sporadic_frames', 'Schedule_tables', 'Signal_encoding_types', 'Signal_representation', 'Node_composition',
  'master', 'slaves', 'time_base', 'jitter', 'max_header_length', 'response_tolerance', 'configurable_frames', 'LIN_protocol', 'product_id', 'P2_min', 'ST_min',
  'N_As_timeout', 'N_Cr_timeout', 'initial_NAD', 'configured_NAD', 'logical_value', 'physical_value', 'bcd_value', 'ascii_value', 'MasterReq', 'SlaveResp',
  'MasterReq_Data', 'SlaveResp_Data', 'delay', 'AssignNAD', 'ConditionalChangeNAD', 'DataDump', 'SaveConfiguration', 'AssignFrameIdRange', 'FreeFormat', 'AssignFrameId'];

export const ldf = wordParser({
  line: ['//'], block: ['/*', '*/'],
  words: { keyword: KEYWORDS, atom: ['ms', 'kbps', 'V'] },
  classify: (word, _state, stream) => (/^\s*\{/.test(stream.string.slice(stream.pos)) && !KEYWORDS.includes(word) ? 'typeName' : undefined),
});
