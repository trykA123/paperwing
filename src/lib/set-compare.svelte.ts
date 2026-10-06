import { CompareState, problem } from './compare-state.svelte';
import type { CompareEndpoint, CompareOptions, CompareRef, CompareResult, CompareSnapshot, SetItem } from './api';

export type SetCompareRow = {
  itemId: string; folder: string; name: string; state: 'queued' | 'comparing' | 'ready' | 'cancelled' | 'failed' | 'missingBoth' | CompareResult['status'];
  left: CompareEndpoint; right: CompareEndpoint; snapshot: CompareSnapshot | null; message: string; added: number | null; removed: number | null;
};

export class SetCompareState {
  rows = $state<SetCompareRow[]>([]);
  busy = $state(false);
  stale = $state(false);
  left = $state<CompareRef>({ kind: 'head' });
  right = $state<CompareRef>({ kind: 'workingTree' });
  options = $state<CompareOptions>({ normalizeEol: true, ignoreWhitespace: false });
  private revision = 0;
  private active = new Set<CompareState>();
  constructor(public setId: string, public items: { id: string; folder: string; name: string }[]) {}
  async cancel() {
    ++this.revision; this.busy = false;
    this.rows = this.rows.map(row => row.state === 'queued' || row.state === 'comparing' ? { ...row, state: 'cancelled', message: 'Cancelled' } : row);
    await Promise.all([...this.active].map(async comparison => { await comparison.cancel(); await comparison.close(); }));
  }
  async run(items: SetItem[]) {
    await this.cancel();
    const revision = ++this.revision;
    this.items = items.map(item => ({ id: item.id, folder: item.folder || item.name, name: item.name }));
    const left: CompareRef = JSON.parse(JSON.stringify(this.left)), right: CompareRef = JSON.parse(JSON.stringify(this.right));
    const options: CompareOptions = JSON.parse(JSON.stringify(this.options));
    this.rows = this.items.map(item => ({ itemId: item.id, folder: item.folder, name: item.name, state: 'queued',
      left: { setId: this.setId, itemId: item.id, reference: left }, right: { setId: this.setId, itemId: item.id, reference: right },
      snapshot: null, message: '', added: null, removed: null }));
    this.busy = true; this.stale = false;
    let next = 0;
    const worker = async () => {
      while (revision === this.revision && next < this.rows.length) {
        const index = next++, row = this.rows[index];
        const comparison = new CompareState(); this.active.add(comparison);
        this.rows[index] = { ...row, state: 'comparing' };
        try {
          await comparison.open(row.left, row.right, options);
          if (revision !== this.revision) return;
          if (comparison.snapshot) {
            await comparison.loadAllFiles();
            if (revision !== this.revision) return;
            if (comparison.error) throw comparison.error;
            let added = 0, removed = 0, known = true;
            for (const file of comparison.files) {
              if (file.left?.kind === 'directory' || file.right?.kind === 'directory' || file.displayStatus === 'same') continue;
              if (!file.displayLines) { known = false; continue; }
              added += file.displayLines.added; removed += file.displayLines.removed;
            }
            this.rows[index] = { ...row, state: 'ready', snapshot: JSON.parse(JSON.stringify(comparison.snapshot)), added: known ? added : null, removed: known ? removed : null };
          } else {
            const failure = comparison.result;
            let state: SetCompareRow['state'] = failure && failure.status !== 'ready' ? failure.status : 'failed';
            if (state === 'missingLeft') {
              await comparison.close();
              await comparison.open(row.right, row.right, options);
              if (revision !== this.revision) return;
              if (comparison.result?.status === 'missingLeft' || comparison.result?.status === 'missingRight') state = 'missingBoth';
            }
            this.rows[index] = { ...row, state,
              message: comparison.error?.message ?? (failure && failure.status !== 'ready' ? failure.problem.message : 'Comparison failed') };
          }
        } catch (error) {
          if (revision === this.revision) this.rows[index] = { ...row, state: 'failed', message: problem(error).message };
        } finally { await comparison.close(); this.active.delete(comparison); }
      }
    };
    try { await Promise.all([worker(), worker()]); }
    finally { if (revision === this.revision) this.busy = false; }
  }
}