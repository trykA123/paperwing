import { invokeComparison } from './compare-response';
import type { CompareContent, CompareEntryKind } from './api';

type ContentRequest = {
  id: string;
  generation: number;
  fileId: string;
  side: 'left' | 'right';
  kind: CompareEntryKind;
};

export async function readComparisonContent(request: ContentRequest): Promise<CompareContent> {
  const { kind, ...args } = request;
  const buffer = await invokeComparison<ArrayBuffer>('comparison_content', args);
  if (!(buffer instanceof ArrayBuffer)) throw new Error('Invalid comparison content response');
  const bytes = new Uint8Array(buffer);
  let binary = bytes.includes(0);
  if (!binary) {
    try { new TextDecoder('utf-8', { fatal: true }).decode(bytes); }
    catch { binary = true; }
  }
  return { generation: request.generation, side: request.side, kind, bytes, binary };
}
