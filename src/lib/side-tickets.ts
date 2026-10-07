import { api, type EditFile } from './api';
import type { Side } from './editor';
import { readContent, type SideContent } from './text-format';

export const MIXED_EOL_REASON = 'Mixed line endings: editing would change them.';

export type TicketTarget = { sessionId: string; generation: number; fileId: string };
export type ReopenedSide = { ticket: EditFile | null; content: SideContent; reason: string | null };

/** Reopens an edit ticket after the file changed on disk. A side that now has mixed line endings gets no ticket. */
export async function reopenSide(target: TicketTarget, side: Side, previous: EditFile): Promise<ReopenedSide> {
  await api.editClose(previous.ticket);
  const ticket = await api.editOpen(target.sessionId, target.generation, target.fileId, side);
  const content = readContent(ticket.bytes);
  if (content.format.editable) return { ticket, content, reason: null };
  await api.editClose(ticket.ticket);
  return { ticket: null, content, reason: MIXED_EOL_REASON };
}
