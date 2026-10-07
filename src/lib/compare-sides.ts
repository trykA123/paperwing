import { api, type CompareEndpoint, type CompareFile, type EditFile } from './api';
import { engineFor, type EngineKind } from './editor';
import { app } from './state.svelte';
import { readContent, type SideContent } from './text-format';

export const MIXED_EOL_REASON = 'Mixed line endings: editing would change them.';

export type SideRequest = { sessionId: string; generation: number; fileId: string; file: CompareFile; endpoints: CompareEndpoint[]; alive: () => boolean; opened: Set<string> };
export type OpenedSides = { kind: EngineKind; contents: SideContent[]; tickets: (EditFile | null)[]; reasons: (string | null)[] };
export type SidesResult = { status: 'ready'; sides: OpenedSides } | { status: 'unsupported'; message: string } | { status: 'cancelled' };

const NAMES = ['left', 'right'] as const;

async function openTicket(request: SideRequest, index: number, reasons: (string | null)[]): Promise<EditFile | null> {
  const capability = app.endpointCapability(request.endpoints[index]!, 'edit');
  if (!capability.supported) { reasons[index] = capability.reason; return null; }
  try { return await api.editOpen(request.sessionId, request.generation, request.fileId, NAMES[index]!); }
  catch (reason) { reasons[index] = String(reason); return null; }
}

async function openTickets(request: SideRequest, contents: SideContent[], reasons: (string | null)[]): Promise<(EditFile | null)[] | null> {
  const tickets: (EditFile | null)[] = [null, null];
  await app.probeEndpoints(request.endpoints);
  if (!request.alive()) return null;
  for (const index of [0, 1]) {
    if (request.endpoints[index]!.reference.kind !== 'workingTree') continue;
    if (!contents[index]!.format.editable) { reasons[index] = MIXED_EOL_REASON; continue; }
    const ticket = await openTicket(request, index, reasons);
    if (!ticket) continue;
    if (!request.alive()) { void api.editClose(ticket.ticket).catch(() => {}); return null; }
    request.opened.add(ticket.ticket); tickets[index] = ticket;
  }
  return tickets;
}

async function dropMixed(request: SideRequest, contents: SideContent[], tickets: (EditFile | null)[], reasons: (string | null)[]) {
  for (const index of [0, 1]) {
    const ticket = tickets[index];
    if (!ticket) continue;
    contents[index] = readContent(ticket.bytes);
    if (contents[index]!.format.editable) continue;
    await api.editClose(ticket.ticket);
    request.opened.delete(ticket.ticket); tickets[index] = null; reasons[index] = MIXED_EOL_REASON;
  }
}

export async function openSides(request: SideRequest): Promise<SidesResult> {
  const { file } = request;
  const fetched = await Promise.all(NAMES.map(side => file[side] ? api.comparisonContent(request.sessionId, request.generation, request.fileId, side, file[side]!.kind) : null));
  if (!request.alive()) return { status: 'cancelled' };
  if (fetched.some(content => content && (content.binary || content.kind !== 'file'))) {
    return { status: 'unsupported', message: 'Binary, linked, or repository content is read-only. Use whole-file copy where supported.' };
  }
  const kind = engineFor(fetched.map(content => content?.bytes.length ?? 0));
  const contents = fetched.map(content => readContent(content?.bytes ?? []));
  const reasons: (string | null)[] = [null, null];
  if (kind === 'viewer') return { status: 'ready', sides: { kind, contents, tickets: [null, null], reasons } };
  const tickets = await openTickets(request, contents, reasons);
  if (!tickets || !request.alive()) return { status: 'cancelled' };
  await dropMixed(request, contents, tickets, reasons);
  return request.alive() ? { status: 'ready', sides: { kind, contents, tickets, reasons } } : { status: 'cancelled' };
}
