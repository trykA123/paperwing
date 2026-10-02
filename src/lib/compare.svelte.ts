import {
    api, type CompareCommit, type CompareContent, type CompareEndpoint, type CompareFile,
    type CompareOptions, type CompareProblem,
    type CompareRef,
    type CompareResult, type CompareSnapshot,
    type SetItem
} from './api';

const defaults: CompareOptions = { normalizeEol: false, ignoreWhitespace: false };

function problem(error: unknown): CompareProblem {
  if (typeof error === 'object' && error !== null && 'kind' in error && 'message' in error) {
    return error as CompareProblem;
  }
  return { kind: 'ipcError', side: null, message: String(error) };
}

export class CompareState {
  editorGuards = new Map<string, () => Promise<boolean>>();
  async guardEditors() {
    for (const guard of this.editorGuards.values()) if (!await guard()) return false;
    return true;
  }
  id = $state<string | null>(null);
  result = $state<CompareResult | null>(null);
  snapshot = $state<CompareSnapshot | null>(null);
  files = $state<CompareFile[]>([]);
  content = $state<CompareContent | null>(null);
  commits = $state<CompareCommit[]>([]);
  error = $state<CompareProblem | null>(null);
  busy = $state(false);
  selectedId = $state<string | null>(null);
  filter = $state<'all' | 'differences' | 'same' | 'orphans'>('all');
  query = $state('');
  excludes = $state('*.orig; build/; .vs/');
  expanded = $state<string[]>([]);
  mode = $state<'files' | 'commits'>('files');
  options = $state<CompareOptions>({ normalizeEol: true, ignoreWhitespace: false });
  private revision = 0;
  private pageRequest = 0;
  private contentRequest = 0;
  private commitRequest = 0;

  private reset() {
    this.result = null; this.snapshot = null; this.files = []; this.content = null; this.commits = []; this.error = null;
  }

  async open(left: CompareEndpoint, right: CompareEndpoint, options: CompareOptions = defaults) {
    if (this.editorGuards.size && !await this.guardEditors()) return;
    const revision = ++this.revision;
    const previous = this.id;
    this.id = null; this.reset(); this.busy = true;
    try {
      if (previous) await api.closeComparison(previous);
      const opened = await api.openComparison(left, right);
      if (revision !== this.revision) { await api.closeComparison(opened.id); return; }
      this.id = opened.id;
      await this.refresh(options);
    } catch (error) {
      if (revision === this.revision) this.error = problem(error);
    } finally {
      if (revision === this.revision) this.busy = false;
    }
  }

  async refresh(options: CompareOptions = this.snapshot?.options ?? defaults) {
    if (this.editorGuards.size && !await this.guardEditors()) return;
    const id = this.id;
    if (!id) return;
    const revision = ++this.revision;
    this.reset(); this.busy = true;
    try {
      const result = await api.refreshComparison(id, options);
      if (revision !== this.revision || id !== this.id) return;
      this.result = result;
      this.snapshot = result.status === 'ready' ? result.snapshot : null;
    } catch (error) {
      if (revision === this.revision && id === this.id) this.error = problem(error);
    } finally {
      if (revision === this.revision && id === this.id) this.busy = false;
    }
  }

  async loadFiles(offset = 0, limit = 512) {
    const snapshot = this.snapshot;
    if (!snapshot) return;
    const revision = this.revision, request = ++this.pageRequest;
    try {
      const files = await api.comparisonFiles(snapshot.id, snapshot.generation, offset, limit);
      if (revision === this.revision && request === this.pageRequest) this.files = files;
    } catch (error) {
      if (revision === this.revision && request === this.pageRequest) this.error = problem(error);
    }
  }

  async loadAllFiles() {
    const snapshot = this.snapshot;
    if (!snapshot) return;
    const revision = this.revision, request = ++this.pageRequest;
    const files: CompareFile[] = [];
    try {
      for (let offset = 0; offset < snapshot.fileCount; offset += 512) {
        const page = await api.comparisonFiles(snapshot.id, snapshot.generation, offset, 512);
        if (revision !== this.revision || request !== this.pageRequest) return;
        files.push(...page);
        if (page.length < 512) break;
      }
      this.files = files;
      this.selectedId = null;
      this.expanded = files.filter(file => file.left?.kind === 'directory' || file.right?.kind === 'directory').map(file => file.path);
    } catch (error) {
      if (revision === this.revision && request === this.pageRequest) this.error = problem(error);
    }
  }

  async loadAllCommits() {
    const snapshot = this.snapshot;
    if (!snapshot?.history.available) return;
    const revision = this.revision, request = ++this.commitRequest;
    const commits: CompareCommit[] = [];
    try {
      for (let offset = 0; offset < (snapshot.history.leftCount ?? 0) + (snapshot.history.rightCount ?? 0); offset += 200) {
        const page = await api.comparisonCommits(snapshot.id, snapshot.generation, offset, 200);
        if (revision !== this.revision || request !== this.commitRequest) return;
        commits.push(...page);
        if (page.length < 200) break;
      }
      this.commits = commits;
    } catch (error) {
      if (revision === this.revision && request === this.commitRequest) this.error = problem(error);
    }
  }

  async loadContent(fileId: string, side: 'left' | 'right') {
    const snapshot = this.snapshot;
    if (!snapshot) return;
    const revision = this.revision, request = ++this.contentRequest;
    this.content = null;
    try {
      const content = await api.comparisonContent(snapshot.id, snapshot.generation, fileId, side);
      if (revision === this.revision && request === this.contentRequest && content.generation === snapshot.generation) this.content = content;
    } catch (error) {
      if (revision === this.revision && request === this.contentRequest) this.error = problem(error);
    }
  }

  async loadCommits(offset = 0, limit = 200) {
    const snapshot = this.snapshot;
    if (!snapshot?.history.available) { this.commits = []; return; }
    const revision = this.revision, request = ++this.commitRequest;
    try {
      const commits = await api.comparisonCommits(snapshot.id, snapshot.generation, offset, limit);
      if (revision === this.revision && request === this.commitRequest) this.commits = commits;
    } catch (error) {
      if (revision === this.revision && request === this.commitRequest) this.error = problem(error);
    }
  }

  async cancel() {
    if (this.editorGuards.size && !await this.guardEditors()) return;
    const id = this.id;
    ++this.revision; this.reset(); this.busy = false;
    if (id) await api.cancelComparison(id);
  }

  async close() {
    if (this.editorGuards.size && !await this.guardEditors()) return;
    const id = this.id;
    ++this.revision; this.id = null; this.reset(); this.busy = false;
    if (id) await api.closeComparison(id);
  }
}

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