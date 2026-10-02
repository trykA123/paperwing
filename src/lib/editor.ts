export type TextFormat = { text: string; bom: boolean; eol: '\n' | '\r\n' | '\r'; editable: boolean };

export function decodeText(bytes: number[] | Uint8Array): TextFormat {
  const input = Uint8Array.from(bytes);
  if (input.includes(0)) throw new Error('Binary files cannot be edited as text.');
  const bom = input[0] === 239 && input[1] === 187 && input[2] === 191;
  const raw = new TextDecoder('utf-8', { fatal: true }).decode(bom ? input.subarray(3) : input);
  const endings = new Set(raw.match(/\r\n|\r|\n/g) ?? []);
  const eol = (endings.values().next().value ?? '\n') as TextFormat['eol'];
  return { text: raw.replace(/\r\n|\r/g, '\n'), bom, eol, editable: endings.size <= 1 };
}

export function encodeText(text: string, format: TextFormat): number[] {
  if (!format.editable) throw new Error('Mixed line endings require a byte-preserving file copy.');
  return Array.from(new TextEncoder().encode((format.bom ? '\uFEFF' : '') + text.replace(/\r\n|\r|\n/g, format.eol)));
}

export function copyHunk(source: string, destination: string, sourceStart: number, sourceEnd: number, destinationStart: number, destinationEnd: number): string {
  const sourceLines = source.split('\n'), destinationLines = destination.split('\n');
  const start = destinationEnd === 0 ? destinationStart : destinationStart - 1;
  const count = destinationEnd === 0 ? 0 : destinationEnd - destinationStart + 1;
  if (![sourceStart, sourceEnd, destinationStart, destinationEnd].every(Number.isInteger)
      || start < 0 || start > destinationLines.length || count < 0 || start + count > destinationLines.length
      || sourceStart < 0 || sourceEnd < 0 || sourceEnd > sourceLines.length || (sourceEnd > 0 && sourceStart < 1)) {
    throw new Error('The diff mapping is obsolete or invalid.');
  }
  const incoming = sourceEnd === 0 ? [] : sourceLines.slice(sourceStart - 1, sourceEnd);
  destinationLines.splice(start, count, ...incoming);
  return destinationLines.join('\n');
}