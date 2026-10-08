(() => {
  const enc = new TextEncoder();
  const SAMPLES = {
    'ecu.arxml': ['<?xml version="1.0" encoding="UTF-8"?>', '<AUTOSAR xmlns="http://autosar.org/schema/r4.0">', '  <AR-PACKAGES>', '    <AR-PACKAGE>', '      <SHORT-NAME>Signals</SHORT-NAME>',
      '      <ELEMENTS>', '        <I-SIGNAL UUID="a1b2">', '          <SHORT-NAME>VehicleSpeed</SHORT-NAME>', '          <LENGTH>16</LENGTH>', '        </I-SIGNAL>', '      </ELEMENTS>', '    </AR-PACKAGE>', '  </AR-PACKAGES>', '</AUTOSAR>'],
    'config.m4': ['dnl Configuration macros', 'define(`PROJECT_NAME\', `skein\')dnl', 'ifelse(`$1\', `\', `default\', `$1\')', 'AC_DEFUN([AX_CHECK_FLAG], [', '  AC_MSG_CHECKING([for $1])', '  divert(-1)', '  include(`common.m4\')', '])', '# plain comment', 'changequote([, ])'],
    'powertrain.a2l': ['ASAP2_VERSION 1 71', '/begin PROJECT Powertrain "Engine control"', '  /begin MODULE ECU ""', '    /begin MEASUREMENT EngineSpeed "Crankshaft speed" UWORD NO_COMPU 1 100 0 8000', '      ECU_ADDRESS 0x40001000', '      FORMAT "%6.1"', '    /end MEASUREMENT', '    /begin CHARACTERISTIC Gain "Gain" VALUE 0x80002000 DAMOS 0 NO_COMPU 0.0 10.5', '    /end CHARACTERISTIC', '  /end MODULE', '/end PROJECT'],
    'body.dbc': ['VERSION "1.0"', '', 'NS_ :', '  CM_', '  BA_DEF_', '', 'BS_:', 'BU_: Gateway Engine', '', 'BO_ 500 EngineData: 8 Engine', ' SG_ EngineSpeed : 0|16@1+ (0.25,0) [0|16383] "rpm" Gateway', ' SG_ Torque : 16|12@1- (1,-500) [-500|1500] "Nm" Gateway', '', 'CM_ SG_ 500 EngineSpeed "Crankshaft speed";', 'VAL_ 500 Torque 4095 "invalid" ;'],
    'node.can': ['/*@@includes: */', 'includes { #include "common.cin" }', 'variables {', '  msTimer cycle;', '  int counter = 0;', '  message EngineData msg;', '}', 'on start { setTimer(cycle, 100); write("started"); }', 'on message EngineData {', '  if (this.EngineSpeed > 3000) counter++;', '  else output(msg);', '}', 'on timer cycle { setTimer(cycle, 100); }'],
    'memory.ld': ['/* Linker script */', 'ENTRY(Reset_Handler)', 'MEMORY {', '  FLASH (rx) : ORIGIN = 0x08000000, LENGTH = 512K', '  RAM (rwx)  : ORIGIN = 0x20000000, LENGTH = 128K', '}', 'SECTIONS {', '  .text : {', '    KEEP(*(.isr_vector))', '    *(.text*)', '    . = ALIGN(4);', '    _etext = .;', '  } > FLASH', '  .data : AT(_etext) { *(.data*) } > RAM', '}'],
    'app.s19': ['S0150000746573742E73313900000000000000000000A2', 'S1137AF00A0A0D0000000000000000000000000061', 'S1137B000000000000000000000000000000000079', 'S1137B1000000000000000000000000000000000E9', 'S1137B200102030405060708090A0B0C0D0E0F1004', 'S5030004F8', 'S9037AF00A'],
    'plant.m': ['% Plant model', 'function y = plant(u, k)', '  persistent x;', "  if isempty(x), x = 0; end", "  x = x + k * (u - x);", "  y = x; % output", "  disp('step');", 'end', 'for i = 1:10', '  y(i) = plant(i, 0.5);', 'end'],
    'engine.c': ['#include <stdint.h>', '#include "Rte_Engine.h"', '', '#define LIMIT 0x7FFFu', '', 'static uint16_t Engine_Scale(uint16_t raw)', '{', '    /* scale to rpm */', '    uint32_t value = (uint32_t)raw * 25u;', '    return (value > LIMIT) ? LIMIT : (uint16_t)value;', '}', '', 'void Engine_MainFunction(void)', '{', '    const char *tag = "engine";', '    Rte_Write_Speed(Engine_Scale(Rte_Read_Raw()));', '}'],
    'notes.zzz': ['<?xml version="1.0"?>', '<root>', '  <item id="1">one</item>', '  <item id="2">two</item>', '</root>'],
  };
  const edits = lines => { const out = lines.map((line, i) => (i === 1 ? line.replace(/(\w)(\W*)$/, '$1x$2') : i === 5 ? line + ' ' : line)); out.splice(3, 0, lines[2]); out.splice(8, 1); return out; };
  const name = window.__FX.file;
  const left = SAMPLES[name], right = edits(left);
  const bytes = { left: enc.encode(left.join('\n')), right: enc.encode(right.join('\n')) };
  const entry = source => ({ kind: 'file', size: 1200, modifiedMs: Date.now(), reason: null, source });
  const files = [{ id: 'f0', path: name, left: entry('commitBlob'), right: entry('workingTree'), rawStatus: 'different', displayStatus: 'different',
    rawLines: { added: 2, removed: 2 }, displayLines: { added: 2, removed: 2 }, binary: false, rename: null, reason: null }];
  const base = window.__TAURI_INTERNALS__.invoke;
  window.__settings = [];
  window.__TAURI_INTERNALS__.invoke = async (command, args = {}) => {
    if (command === 'comparison_open') return base(command, { ...args, left: args.right });
    if (command === 'comparison_files') return args.offset ? [] : files;
    if (command === 'comparison_content') return bytes[args.side].buffer.slice(0);
    if (command === 'file_edit_open') return { ticket: `ticket-${args.side}`, bytes: Array.from(bytes[args.side]), exists: true };
    if (command === 'file_edit_close') return true;
    if (command === 'save_settings') window.__settings.push(JSON.parse(JSON.stringify(args)));
    return base(command, args);
  };
})();
