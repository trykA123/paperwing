import { benchmarkTimer } from './benchmark';
import {
    api, type CompareCommit, type CompareContent, type CompareEndpoint, type CompareFile,
    type CompareOptions, type CompareProblem,
    type CompareResult, type CompareSnapshot
} from './api';

const defaults: CompareOptions = { normalizeEol: false, ignoreWhitespace: false };

export function problem(error: unknown): CompareProblem {
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
  private finishRequest = () => {};
  private firstRender = () => {};
  private finishComparison = () => {};

  rendered() { this.firstRender(); this.finishComparison(); }

  private revision = 0;
  private pageRequest = 0;
  private contentRequest = 0;
  private commitRequest = 0;

  private reset() {
    this.result = null; this.snapshot = null; this.files = []; this.content = null; this.commits = []; this.error = null;
  }

  async open(left: CompareEndpoint, right: CompareEndpoint, options: CompareOptions = defaults) {
    if (this.editorGuards.size && !await this.guardEditors()) return;
    this.finishRequest = benchmarkTimer('ui.request');
    this.firstRender = benchmarkTimer('ui.first-render');
    this.finishComparison = benchmarkTimer('ui.complete');
    const revision = ++this.revision;
    const previous = this.id;
    this.id = null; this.reset(); this.busy = true;
    try {
      if (previous) await api.closeComparison(previous);
      const opened = await api.openComparison(left, right);
      if (revision !== this.revision) { await api.closeComparison(opened.id); return; }
      this.id = opened.id;
      await this.refresh(options);
      this.finishRequest();
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
    const finishFiles = benchmarkTimer('ui.files-ready');
    const files: CompareFile[] = [];
    try {
      for (let offset = 0; offset < snapshot.fileCount; offset += 512) {
        const page = await api.comparisonFiles(snapshot.id, snapshot.generation, offset, 512);
        if (revision !== this.revision || request !== this.pageRequest) return;
        files.push(...page);
        if (page.length < 512) break;
      }
      this.files = files;
      finishFiles();
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
      let row = this.files.find(file => file.id === fileId);
      for (let offset = 0; !row && offset < snapshot.fileCount; offset += 512) {
        const page = await api.comparisonFiles(snapshot.id, snapshot.generation, offset, 512);
        if (revision !== this.revision || request !== this.contentRequest) return;
        row = page.find(file => file.id === fileId);
        if (page.length < 512) break;
      }
      const kind = row?.[side]?.kind;
      if (!kind) throw new Error('Comparison entry unavailable; refresh the comparison');
      const content = await api.comparisonContent(snapshot.id, snapshot.generation, fileId, side, kind);
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

