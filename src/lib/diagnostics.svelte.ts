import { listen } from '@tauri-apps/api/event';
import { api } from './api';
import type { NoticeKind } from './notifications.svelte';

export type DiagnosticsStatus = { sampling: boolean; samples: number };
export type DiagnosticsProgress = { phase: string; completed: number; total: number };
export type DiagnosticsTransport = {
  status: () => Promise<DiagnosticsStatus>;
  preview: () => Promise<string>;
  cancel: () => Promise<boolean>;
  save: () => Promise<boolean>;
  onProgress: (handler: (progress: DiagnosticsProgress) => void) => Promise<() => void>;
};
export type DiagnosticsPhase = 'idle' | 'collecting' | 'ready' | 'saving';

export const tauriDiagnostics: DiagnosticsTransport = {
  status: () => api.diagnosticsStatus(),
  preview: () => api.diagnosticsPreview(),
  cancel: () => api.diagnosticsCancel(),
  save: () => api.diagnosticsExport(),
  onProgress: async handler => listen<DiagnosticsProgress>('diagnostics-progress', event => handler(event.payload)),
};

export function formatByteSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function progressPercent(progress: DiagnosticsProgress | null): number {
  if (!progress || progress.total <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((progress.completed / progress.total) * 100)));
}

const failureText = (error: unknown) => String((error as Error)?.message ?? error);

export class DiagnosticsStore {
  available = $state(false);
  samples = $state(0);
  phase = $state<DiagnosticsPhase>('idle');
  progress = $state<DiagnosticsProgress | null>(null);
  preview = $state('');
  error = $state('');
  readonly bytes = $derived(this.preview ? new TextEncoder().encode(this.preview).length : 0);
  private readonly transport: DiagnosticsTransport;
  private readonly notify: (message: string, kind: NoticeKind) => void;
  private cancelled = false;

  constructor(transport: DiagnosticsTransport, notify: (message: string, kind: NoticeKind) => void) {
    this.transport = transport;
    this.notify = notify;
  }

  async probe(): Promise<void> {
    try {
      const status = await this.transport.status();
      this.samples = status.samples;
      this.available = true;
    } catch {
      this.available = false;
      console.debug('Diagnostics are not part of this build');
    }
  }

  async refreshSamples(): Promise<void> {
    if (!this.available) return;
    const status = await this.transport.status().catch(() => null);
    if (status) this.samples = status.samples;
  }

  async generate(): Promise<void> {
    if (this.phase !== 'idle' && this.phase !== 'ready') return;
    this.phase = 'collecting';
    this.cancelled = false;
    this.error = '';
    this.preview = '';
    this.progress = null;
    let stop: (() => void) | undefined;
    try {
      stop = await this.transport.onProgress(progress => { this.progress = progress; });
      this.preview = await this.transport.preview();
      this.phase = 'ready';
    } catch (error) {
      this.phase = 'idle';
      if (!this.cancelled) this.error = failureText(error);
    } finally {
      stop?.();
      this.progress = null;
      void this.refreshSamples();
    }
  }

  async cancel(): Promise<void> {
    if (this.phase !== 'collecting') return;
    this.cancelled = true;
    try { await this.transport.cancel(); } catch (error) { this.error = failureText(error); }
  }

  async save(): Promise<void> {
    if (this.phase !== 'ready') return;
    this.phase = 'saving';
    this.error = '';
    try {
      if (await this.transport.save()) this.notify('Diagnostics saved', 'success');
    } catch (error) {
      this.error = failureText(error);
      this.preview = '';
      this.phase = 'idle';
      return;
    }
    this.phase = 'ready';
  }
}
