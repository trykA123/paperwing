(() => {
  const scenario = window.__FX.scenario;
  const enc = new TextEncoder();
  const functionBody = i => [
    `static int compute_${i}(const struct frame *f, int limit)`, '{', '\tint total = 0;', `\tfor (int i = 0; i < limit; i++) {`,
    `\t\tif (f->flags[i] & MASK_${i % 7}) total += f->values[i] * ${i % 5 + 1};`, '\t}', '\treturn total;', '}', '',
  ];
  const cFile = () => {
    const lines = ['#include <stdio.h>', '#include "frame.h"', '', '#define MASK_0 0x01'];
    for (let i = 0; i < 18; i++) lines.push(...functionBody(i));
    return lines;
  };
  const edit = lines => {
    const out = [...lines];
    out[10] = '\tint total = 1; /* seed */';
    out.splice(34, 3);
    out.splice(80, 0, '\t/* clamp before summing */', '\tif (limit > MAX_FRAME) limit = MAX_FRAME;', '\tif (limit < 0) limit = 0;');
    out[130] = out[130].replace('total +=', 'total -=');
    return out;
  };
  const join = (lines, eol, bom) => enc.encode((bom ? '\uFEFF' : '') + lines.join(eol));
  const largeLines = () => Array.from({ length: 125000 }, (_, i) => `int value_${i} = compute(${i}, "text-${i % 97}", ${i * 3 % 1013});`);
  const minified = changed => {
    const items = Array.from({ length: 16638 }, (_, i) => ({ id: i, name: `item-${i}`, tags: ['a', 'b', `t${i % 11}`], v: changed && i % 110 === 7 ? i * 2 : i / 2 }));
    return enc.encode(JSON.stringify(items));
  };
  const make = {
    c: () => [join(cFile(), '\n'), join(edit(cFile()), '\n')],
    crlf: () => [join(cFile(), '\r\n', true), join(edit(cFile()), '\r\n', true)],
    mixed: () => { const text = cFile().join('\n').replace('\n#include "frame.h"', '\r\n#include "frame.h"'); return [enc.encode(text), enc.encode(text.replace('int total = 0', 'int total = 5'))]; },
    mb: () => { const left = Array.from({ length: 24000 }, (_, i) => `export const value_${i} = compute(${i}, "text-${i % 97}");`), right = left.map((line, i) => (i % 700 === 350 ? `${line} // changed` : line)); return [join(left, '\n'), join(right, '\n')]; },
    large: () => { const left = largeLines(), right = left.map((line, i) => (i % 1000 === 500 ? line.replace('compute', 'compute_v2') : line)); return [join(left, '\n'), join(right, '\n')]; },
    json: () => [minified(false), minified(true)],
  }[scenario]();
  const sides = { left: make[0], right: make[1] };
  const entry = (kind, source) => ({ kind: 'file', size: 1200, modifiedMs: Date.now(), reason: null, source });
  const path = { c: 'engine.c', crlf: 'frame.c', mixed: 'mixed.c', large: 'values.txt', mb: 'values.ts', json: 'items.json' }[scenario];
  const files = [{ id: 'f0', path, left: entry('file', 'commitBlob'), right: entry('file', 'workingTree'), rawStatus: 'different', displayStatus: 'different',
    rawLines: { added: 4, removed: 2 }, displayLines: { added: 4, removed: 2 }, binary: false, rename: null, reason: null }];
  const base = window.__TAURI_INTERNALS__.invoke;
  window.__saved = [];
  window.__bytes = sides;
  window.__TAURI_INTERNALS__.invoke = async (command, args = {}) => {
    if (command === 'comparison_open' && window.__FX.bothWorking) return base(command, { ...args, left: args.right });
    if (command === 'comparison_open' && window.__FX.leftOnly) return base(command, { ...args, left: args.right, right: args.left });
    if (command === 'comparison_files') return args.offset ? [] : files;
    if (command === 'comparison_content') return sides[args.side].buffer.slice(0);
    if (command === 'file_edit_open') return { ticket: `ticket-${args.side}`, bytes: Array.from(sides[args.side]), exists: true };
    if (command === 'file_edit_close') return true;
    if (command === 'file_save') { window.__saved.push({ ticket: args.ticket, bytes: Array.from(args.bytes) }); return { id: `record-${window.__saved.length}`, root: 'C:\\Dev\\repos', path, existed: true, stage: 'applied', createdAt: Date.now(), warning: null }; }
    return base(command, args);
  };
  window.__openTiming = {};
  document.addEventListener('dblclick', () => { window.__openTiming.start = performance.now(); }, true);
  new MutationObserver(() => {
    if (window.__openTiming.ready || !window.__openTiming.start) return;
    if (document.querySelector('.editor-host .cm-line, .editor-host .vw-row')) window.__openTiming.ready = performance.now();
  }).observe(document, { subtree: true, childList: true });
})();
