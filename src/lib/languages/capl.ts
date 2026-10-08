import { wordParser } from './rules';

export const capl = wordParser({
  line: ['//'], block: ['/*', '*/'], quotes: '"\'',
  words: {
    keyword: ['if', 'else', 'for', 'while', 'do', 'switch', 'case', 'default', 'break', 'continue', 'return', 'on', 'variables', 'includes', 'testcase', 'testfunction',
      'const', 'struct', 'enum', 'this', 'sizeof', 'write', 'writeEx', 'output', 'setTimer', 'setTimerCyclic', 'cancelTimer', 'putValue', 'getValue', 'testStep',
      'testStepPass', 'testStepFail', 'testWaitForTimeout', 'testWaitForSignalMatch'],
    typeName: ['int', 'long', 'dword', 'word', 'byte', 'char', 'float', 'double', 'void', 'qword', 'int64', 'message', 'msTimer', 'timer', 'signal', 'sysvar', 'envvar',
      'diagRequest', 'diagResponse', 'mstimer', 'linMessage', 'pg', 'frFrame'],
    atom: ['start', 'stop', 'key', 'timer', 'envVar', 'sysVar', 'errorFrame', 'busOff', 'preStart', 'preStop', 'stopMeasurement', 'TRUE', 'FALSE', 'NULL'],
  },
  extra: stream => (stream.sol() && stream.match(/^#\s*\w+/) ? 'keyword' : undefined),
});
