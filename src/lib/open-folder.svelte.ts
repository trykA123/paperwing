import { listen } from '@tauri-apps/api/event';
import { api, events } from './api';
import type { DiscoverBatch, DiscoverDone, DiscoverSummary, FoundRepo, LaunchAction, LaunchRequest } from './api';
import type { NoticeKind } from './notifications.svelte';

export type TempSet = {
  id: string; scanId: number; name: string; path: string; repos: FoundRepo[];
  scanning: boolean; capped: DiscoverSummary['capped']; cancelled: boolean; summary: DiscoverSummary | null;
};
export type FolderCompareRequest = { left: string; right: string };
export type ScanEvent = { type: 'batch'; batch: DiscoverBatch } | { type: 'done'; done: DiscoverDone };
export type Notify = (message: string, kind: NoticeKind) => void;
export type Transport = {
  drainRequests: () => Promise<LaunchRequest[]>;
  startScan: (path: string) => Promise<number>;
  cancelScan: (id: number) => Promise<boolean>;
  subscribe: (handlers: { onSignal: () => void; onBatch: (batch: DiscoverBatch) => void; onDone: (done: DiscoverDone) => void }) => Promise<() => void>;
};

export const tauriTransport: Transport = {
  drainRequests: () => api.launchRequest(),
  startScan: path => api.discoverStart(path),
  cancelScan: id => api.discoverCancel(id),
  subscribe: async ({ onSignal, onBatch, onDone }) => {
    const stops = await Promise.all([
      listen(events.launchRequest, onSignal),
      listen<DiscoverBatch>(events.discoverBatch, event => onBatch(event.payload)),
      listen<DiscoverDone>(events.discoverDone, event => onDone(event.payload)),
    ]);
    return () => stops.forEach(stop => stop());
  },
};

export function folderName(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export function describeScan(set: Pick<TempSet, 'name' | 'path' | 'repos' | 'capped'>): string {
  if (set.repos.some(repo => repo.path === set.path)) return `${set.name} is a repository`;
  if (set.repos.length === 0) return set.capped ? `No repositories found in ${set.name} before the scan limit` : `No repositories found in ${set.name}`;
  const noun = set.repos.length === 1 ? 'repository' : 'repositories';
  return `Found ${set.repos.length} ${noun} in ${set.name}${set.capped ? ' (scan stopped at its limit)' : ''}`;
}

export class OpenFolderStore {
  sets = $state<TempSet[]>([]);
  compare = $state<FolderCompareRequest | null>(null);
  private readonly transport: Transport;
  private readonly notify: Notify;
  private readonly early = new Map<number, ScanEvent[]>();
  private readonly known = new Map<number, Set<string>>();
  private readonly dismissed = new Set<number>();
  private stopListening: (() => void) | undefined;
  private active = false;

  constructor(transport: Transport, notify: Notify) {
    this.transport = transport;
    this.notify = notify;
  }

  async start(): Promise<void> {
    this.active = true;
    const stop = await this.transport.subscribe({
      onSignal: () => void this.drain(),
      onBatch: batch => this.apply({ type: 'batch', batch }),
      onDone: done => this.apply({ type: 'done', done }),
    });
    if (!this.active) { stop(); return; }
    this.stopListening = stop;
    await this.drain();
  }

  stop(): void {
    this.active = false;
    this.stopListening?.();
    this.stopListening = undefined;
    for (const set of this.sets) if (set.scanning) void this.transport.cancelScan(set.scanId).catch(() => false);
  }

  async receive(requests: LaunchRequest[]): Promise<void> {
    for (const request of requests) {
      for (const item of request.ignored) this.notify(`Ignored ${item.arg}: ${item.reason}`, 'warn');
      if (request.action) await this.run(request.action);
    }
  }

  async open(path: string): Promise<TempSet | null> {
    try {
      const scanId = await this.transport.startScan(path);
      const set: TempSet = { id: `temp-${scanId}`, scanId, name: folderName(path), path, repos: [], scanning: true, capped: null, cancelled: false, summary: null };
      this.sets.push(set);
      this.known.set(scanId, new Set());
      for (const event of this.early.get(scanId) ?? []) this.apply(event);
      this.early.delete(scanId);
      return this.sets.find(entry => entry.id === set.id) ?? null;
    } catch (reason) {
      this.notify(`Could not scan ${path}: ${reason}`, 'error');
      return null;
    }
  }

  dismiss(id: string): void {
    const set = this.sets.find(entry => entry.id === id);
    if (!set) return;
    if (set.scanning) void this.transport.cancelScan(set.scanId).catch(() => false);
    this.sets = this.sets.filter(entry => entry.id !== id);
    this.known.delete(set.scanId);
    this.dismissed.add(set.scanId);
  }

  apply(event: ScanEvent): void {
    const scanId = event.type === 'batch' ? event.batch.id : event.done.id;
    const set = this.sets.find(entry => entry.scanId === scanId);
    if (!set) {
      if (this.active && !this.dismissed.has(scanId)) this.early.set(scanId, [...(this.early.get(scanId) ?? []), event]);
      return;
    }
    if (event.type === 'batch') this.merge(set, event.batch.repos);
    else this.finish(set, event.done.summary);
  }

  private async drain(): Promise<void> {
    try { await this.receive(await this.transport.drainRequests()); } catch (reason) { this.notify(`Could not read the launch request: ${reason}`, 'error'); }
  }

  private async run(action: LaunchAction): Promise<void> {
    if (action.kind === 'openFolder') { await this.open(action.path); return; }
    this.compare = { left: action.left, right: action.right };
    this.notify('Comparing two folders from the file manager is not available yet.', 'warn');
  }

  private merge(set: TempSet, repos: FoundRepo[]): void {
    const seen = this.known.get(set.scanId) ?? new Set<string>();
    this.known.set(set.scanId, seen);
    for (const repo of repos) {
      if (seen.has(repo.path)) continue;
      seen.add(repo.path);
      set.repos.push(repo);
    }
  }

  private finish(set: TempSet, summary: DiscoverSummary): void {
    set.scanning = false;
    set.summary = summary;
    set.capped = summary.capped;
    set.cancelled = summary.cancelled;
    if (!summary.cancelled) this.notify(describeScan(set), set.capped ? 'warn' : 'info');
  }
}
