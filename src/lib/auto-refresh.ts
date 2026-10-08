import type { WatchReport } from './api';

export const REFRESH_TRAILING_MS = 300;
export const REFRESH_MAX_WAIT_MS = 2000;
const LISTED_FOLDERS = 3;

export type AutoRefreshHost = {
  refresh: (paths: string[]) => Promise<unknown> | void;
  notice: (message: string, kind: 'info' | 'warn') => void;
  watch: (setId: string, roots: string[]) => Promise<WatchReport>;
  unwatch: (setId: string) => Promise<unknown>;
  trailingMs?: number;
  maxWaitMs?: number;
};

export type WatchTargets = ReadonlyMap<string, readonly string[]>;

const ALL = '*';
const signatureOf = (roots: readonly string[]) => [...roots].sort().join('\n');
const plural = (count: number, word: string) => `${count} ${word}${count === 1 ? '' : 's'}`;
const folderName = (path: string) => path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;

export class AutoRefresh {
  #host: AutoRefreshHost;
  #pending = new Set<string>();
  #inFlight = new Set<string>();
  #dirty = new Set<string>();
  #trailing: ReturnType<typeof setTimeout> | undefined;
  #ceiling: ReturnType<typeof setTimeout> | undefined;
  #applied = new Map<string, string>();
  #active = new Set<string>();
  #queue: Promise<void> = Promise.resolve();
  #warned = new Set<string>();
  #noted = new Set<string>();

  constructor(host: AutoRefreshHost) { this.#host = host; }

  get watching() { return this.#active.size; }

  changed(path: string) {
    this.#noted.delete(`lost:${path}`);
    this.#pending.add(path);
    clearTimeout(this.#trailing);
    this.#trailing = setTimeout(() => this.#flush(), this.#host.trailingMs ?? REFRESH_TRAILING_MS);
    this.#ceiling ??= setTimeout(() => this.#flush(), this.#host.maxWaitMs ?? REFRESH_MAX_WAIT_MS);
  }

  sync(targets: WatchTargets): Promise<void> {
    this.#queue = this.#queue.then(() => this.#reconcile(targets));
    return this.#queue;
  }

  failed(reason: string): Promise<void> {
    this.#queue = this.#queue.then(async () => {
      for (const setId of [...this.#active]) await this.#drop(setId);
      this.#applied.clear();
      this.#warn(ALL, `Automatic refresh stopped: ${reason}.`);
    });
    return this.#queue;
  }

  lost(path: string, reason: string) {
    this.#note(`lost:${path}`, `Automatic refresh lost ${folderName(path)}: ${reason}. Refresh local status still works.`, 'warn');
  }

  #flush() {
    clearTimeout(this.#trailing);
    clearTimeout(this.#ceiling);
    this.#trailing = undefined;
    this.#ceiling = undefined;
    const paths = [...this.#pending];
    this.#pending.clear();
    this.#run(paths);
  }

  #run(paths: string[]) {
    const start: string[] = [];
    for (const path of paths) {
      if (this.#inFlight.has(path)) this.#dirty.add(path);
      else start.push(path);
    }
    if (!start.length) return;
    for (const path of start) this.#inFlight.add(path);
    let work: Promise<unknown>;
    try { work = Promise.resolve(this.#host.refresh(start)); } catch (reason) { work = Promise.reject(reason); }
    work.catch(reason => this.#warn('refresh', `Automatic refresh could not read a folder: ${String(reason)}.`)).finally(() => {
      for (const path of start) this.#inFlight.delete(path);
      const again = start.filter(path => this.#dirty.delete(path));
      if (again.length) this.#run(again);
    });
  }

  async #reconcile(targets: WatchTargets) {
    for (const setId of [...this.#applied.keys()]) {
      if (targets.get(setId)?.length) continue;
      this.#applied.delete(setId);
      await this.#drop(setId);
    }
    for (const [setId, roots] of targets) {
      const signature = signatureOf(roots);
      if (!roots.length || this.#applied.get(setId) === signature) continue;
      this.#applied.set(setId, signature);
      await this.#start(setId, roots);
    }
  }

  async #start(setId: string, roots: readonly string[]) {
    try {
      const report = await this.#host.watch(setId, [...roots]);
      this.#active.add(setId);
      this.#warned.delete(setId);
      this.#warned.delete(ALL);
      this.#explain(setId, report);
    } catch (reason) {
      this.#active.delete(setId);
      this.#warn(setId, `${String(reason).replace(/\.$/, '')}.`);
    }
  }

  #explain(setId: string, report: WatchReport) {
    if (report.skipped.length) {
      const names = report.skipped.slice(0, LISTED_FOLDERS).map(entry => folderName(entry.path));
      const more = report.skipped.length > LISTED_FOLDERS ? ` and ${report.skipped.length - LISTED_FOLDERS} more` : '';
      this.#note(`skip:${setId}`, `Automatic refresh skipped ${plural(report.skipped.length, 'folder')}: ${names.join(', ')}${more}. Refresh local status still works for them.`, 'info');
    }
    if (report.bestEffort.length) {
      this.#note(`effort:${setId}`, `${plural(report.bestEffort.length, 'folder')} sit on a network share or in a cloud-sync folder; automatic refresh there is best effort.`, 'info');
    }
  }

  async #drop(setId: string) {
    this.#active.delete(setId);
    this.#noted.delete(`skip:${setId}`);
    this.#noted.delete(`effort:${setId}`);
    try { await this.#host.unwatch(setId); } catch (reason) { this.#warn(ALL, `Automatic refresh could not stop watching a set: ${String(reason)}.`); }
  }

  #warn(key: string, text: string) {
    if (this.#warned.has(key)) return;
    this.#warned.add(key);
    this.#host.notice(`${text} Refresh local status still works.`, 'warn');
  }

  #note(key: string, text: string, kind: 'info' | 'warn') {
    if (this.#noted.has(key)) return;
    this.#noted.add(key);
    this.#host.notice(text, kind);
  }
}
