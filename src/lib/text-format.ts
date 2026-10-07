export type TextFormat = { text: string; bom: boolean; eol: '\n' | '\r\n' | '\r'; editable: boolean };
export type SideContent = { format: TextFormat; original: Uint8Array | null };

export function decodeText(bytes: ArrayLike<number>): TextFormat {
  const input = Uint8Array.from(bytes);
  if (input.includes(0)) throw new Error('Binary files cannot be edited as text.');
  const bom = input[0] === 239 && input[1] === 187 && input[2] === 191;
  const raw = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bom ? input.subarray(3) : input);
  const endings = new Set(raw.match(/\r\n|\r|\n/g) ?? []);
  const eol = (endings.values().next().value ?? '\n') as TextFormat['eol'];
  return { text: raw.replace(/\r\n|\r/g, '\n'), bom, eol, editable: endings.size <= 1 };
}

export function encodeText(text: string, format: TextFormat): number[] {
  if (!format.editable) throw new Error('Mixed line endings require a byte-preserving file copy.');
  return Array.from(new TextEncoder().encode((format.bom ? '\uFEFF' : '') + text.replace(/\r\n|\r|\n/g, format.eol)));
}

/** Mixed line endings cannot be restored from a normalised buffer, so those sides keep their original bytes and stay read-only. */
export function readContent(bytes: ArrayLike<number>): SideContent {
  const format = decodeText(bytes);
  return { format, original: format.editable ? null : Uint8Array.from(bytes) };
}

export function contentBytes(content: SideContent, text: string): number[] {
  if (!content.format.editable) {
    if (text !== content.format.text) throw new Error('A file with mixed line endings cannot be edited.');
    return Array.from(content.original!);
  }
  return encodeText(text, content.format);
}

export const EMPTY_CONTENT: SideContent = { format: { text: '', bom: false, eol: '\n', editable: true }, original: null };

export function formatLabel(format: TextFormat | undefined): string {
  return format?.eol === '\r\n' ? 'UTF-8 · CRLF' : format?.eol === '\r' ? 'UTF-8 · CR' : 'UTF-8 · LF';
}

export function utf8Length(text: string): number {
  return new TextEncoder().encode(text).byteLength;
}

export function contentFromText(text: string): SideContent {
  return { format: { text: text.replace(/\r\n?/g, '\n'), bom: false, eol: '\n', editable: true }, original: null };
}
