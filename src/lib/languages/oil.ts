import { wordParser } from './rules';

export const oil = wordParser({
  line: ['//'], block: ['/*', '*/'],
  words: {
    keyword: ['OIL_VERSION', 'IMPLEMENTATION', 'CPU', 'OS', 'TASK', 'ISR', 'ALARM', 'EVENT', 'RESOURCE', 'COUNTER', 'APPMODE', 'MESSAGE', 'COM', 'NM', 'IPDU', 'NETWORKMESSAGE', 'SCHEDULETABLE'],
    propertyName: ['STATUS', 'PRIORITY', 'SCHEDULE', 'ACTIVATION', 'AUTOSTART', 'STACKSIZE', 'MASK', 'ACTION', 'ALARMTIME', 'CYCLETIME',
      'MAXALLOWEDVALUE', 'TICKSPERBASE', 'MINCYCLE', 'CATEGORY', 'APP_SRC', 'ENTRY', 'SHUTDOWNHOOK', 'STARTUPHOOK', 'ERRORHOOK', 'PRETASKHOOK', 'POSTTASKHOOK', 'USERESSCHEDULER'],
    atom: ['TRUE', 'FALSE', 'NON', 'FULL', 'AUTO', 'STANDARD', 'EXTENDED', 'ACTIVATETASK', 'SETEVENT', 'ALARMCALLBACK', 'SOFTWARE', 'HARDWARE', 'STD', 'EXT'],
  },
  extra: stream => (stream.sol() && stream.match(/^#\s*\w+/) ? 'keyword' : undefined),
  classify: (word, _state, stream) => (/^\s*=/.test(stream.string.slice(stream.pos)) && /^[A-Z_]+$/.test(word) ? 'propertyName' : undefined),
});
