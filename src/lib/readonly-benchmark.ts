import { api, type CompareFile, type CompareSnapshot, type EditFile } from './api';
import { benchmarkFinished } from './benchmark';
import type { CompareEditor } from './editor';

type Target = { sessionId: string; generation: number; fileId: string };
type Proof = { editor: CompareEditor; host: HTMLElement; tickets: (EditFile | null)[]; snapshot: CompareSnapshot; files: CompareFile[]; platform: string };

export async function verifyReadonly(target: Target, proof: Proof) {
  const { editor, host, tickets, snapshot } = proof;
  const readonly = editor.isReadOnly('right'), originalEditable = !editor.isReadOnly('left');
  if (!host.isConnected || !host.getBoundingClientRect().height || !readonly || originalEditable || tickets.some(Boolean)) throw new Error('Native editor is not read-only.');
  const attempts: [string, () => Promise<unknown>][] = [
    ['file_edit_open', () => api.editOpen(target.sessionId, target.generation, target.fileId, 'right')],
    ['file_edit_close', () => api.editClose('unsupported')], ['file_save', () => api.fileSave('unsupported', [])],
    ['copy_preview', () => api.copyPreview(target.sessionId, target.generation, target.fileId, 'right')],
    ['copy_apply', () => api.copyApply('unsupported', true)], ['copy_cancel', () => api.copyCancel('unsupported')],
    ['recovery_list', () => api.recoveryList()], ['recovery_undo', () => api.recoveryUndo('unsupported')],
    ['recovery_cleanup', () => api.recoveryCleanup([], true)], ['recovery_resolve', () => api.recoveryResolve('unsupported', true)],
  ];
  const refusals = [];
  for (const [command, run] of attempts) {
    let reason = '';
    try { await run(); } catch (error) { reason = String(error); }
    if (!reason.includes('Linux') || reason.toLowerCase().includes('not found')) throw new Error(`Native ${command} did not refuse explicitly.`);
    refusals.push({ command, reason });
  }
  await benchmarkFinished(snapshot, proof.files, { scenario: 'linux-read-only', platform: proof.platform,
    connected: true, readOnly: readonly, originalEditable, ticketCount: tickets.filter(Boolean).length,
    workingSide: snapshot.right.endpoint.reference.kind === 'workingTree' ? 'right' : 'invalid',
    originalLength: editor.getText('left').length, modifiedLength: editor.getText('right').length, refusals });
}
