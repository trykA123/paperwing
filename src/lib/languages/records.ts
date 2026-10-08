import type { StreamParser } from '@codemirror/language';

type Piece = readonly [number, string | null];
type RecordState = { queue: Piece[] };

export function recordParser(layout: (line: string) => Piece[] | null): StreamParser<RecordState> {
  return {
    name: 'record',
    startState: () => ({ queue: [] }),
    token(stream, state) {
      if (stream.sol()) {
        const line = stream.string.trimEnd();
        state.queue = layout(line) ?? [[line.length, 'invalid']];
      }
      while (state.queue[0]?.[0] === 0) state.queue.shift();
      const piece = state.queue.shift();
      if (!piece) { stream.skipToEnd(); return null; }
      const left = stream.string.trimEnd().length - stream.pos;
      if (left <= 0) { stream.skipToEnd(); return null; }
      for (let i = 0; i < Math.min(piece[0], left); i++) stream.next();
      return piece[1];
    },
  };
}

const HEX = /^[\da-fA-F]*$/;

export function layoutIntelHex(line: string): Piece[] | null {
  if (line.length < 11 || line[0] !== ':' || !HEX.test(line.slice(1)) || line.length % 2 === 0) return null;
  return [[1, 'punctuation'], [2, 'number'], [4, 'propertyName'], [2, 'keyword'], [line.length - 11, null], [2, 'atom']];
}

const ADDRESS_BYTES: Record<string, number> = { 0: 2, 1: 2, 2: 3, 3: 4, 5: 2, 6: 3, 7: 4, 8: 3, 9: 2 };

export function layoutSRecord(line: string): Piece[] | null {
  const kind = /^S(\d)/.exec(line)?.[1];
  if (!kind || !(kind in ADDRESS_BYTES) || !HEX.test(line.slice(2)) || line.length < 4) return null;
  const address = ADDRESS_BYTES[kind]! * 2;
  return [[2, 'keyword'], [2, 'number'], [address, 'propertyName'], [Math.max(0, line.length - 4 - address - 2), null], [2, 'atom']];
}
