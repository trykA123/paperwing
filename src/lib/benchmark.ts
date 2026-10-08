import { invoke } from '@tauri-apps/api/core';
import type { CompareEndpoint, CompareFile, CompareSnapshot, RootSupport } from './api';

export const benchmarkEnabled = import.meta.env.VITE_SKEIN_BENCHMARK_FIXTURE === '1';
const timingEnabled = benchmarkEnabled || import.meta.env.VITE_SKEIN_BENCHMARK === '1';
export type BenchmarkPhase = 'ui.request' | 'ui.files-ready' | 'ui.first-render' | 'ui.complete' | 'editor.import' | 'editor.construct' | 'editor.diff';
const pending = new Set<Promise<unknown>>();
let failed = false;

export function benchmarkTimer(phase: BenchmarkPhase) {
  if (!timingEnabled) return () => {};
  const started = performance.now();
  let ended = false;
  return () => {
    if (ended) return;
    ended = true;
    const request = invoke('benchmark_record', { event: { phase, durationMs: performance.now() - started } })
      .catch(() => { failed = true; console.error('Benchmark event could not be recorded'); })
      .finally(() => pending.delete(request));
    pending.add(request);
  };
}

let rootProbeProof: Record<string, unknown> | undefined;

export async function benchmarkPlan() {
  const plan = await invoke<{ left: CompareEndpoint; right: CompareEndpoint; scenario?: 'linux-read-only' | 'linux-root-probe'; probeRoot?: string; expectBlockedRootProbe?: boolean }>('benchmark_plan');
  if (plan.scenario === 'linux-root-probe') {
    if (!plan.probeRoot) throw new Error('Native root-probe fixture is missing');
    let settled = false, frames = 0, frame = 0;
    const tick = () => { if (!settled) { frames++; frame = requestAnimationFrame(tick); } };
    frame = requestAnimationFrame(tick);
    const started = performance.now();
    const probe = invoke<RootSupport>('probe_root', { root: plan.probeRoot }).finally(() => { settled = true; });
    await new Promise(resolve => setTimeout(resolve, 50));
    await invoke('benchmark_snapshot');
    const fastBeforeProbeDone = !settled;
    const support = await probe;
    cancelAnimationFrame(frame);
    const elapsedMs = performance.now() - started;
    const expectedResponsive = !plan.expectBlockedRootProbe;
    if (fastBeforeProbeDone !== expectedResponsive || !support.valid || (expectedResponsive && frames < 2) || elapsedMs < 900) throw new Error('Native root probe IPC or rendering did not match the control');
    rootProbeProof = { scenario: 'linux-root-probe', platform: 'linux', fastBeforeProbeDone, expectedResponsive, rootValid: support.valid, frames, elapsedMs };
  }
  return plan;
}

export async function benchmarkFinished(snapshot: CompareSnapshot, files: CompareFile[], proof?: Record<string, unknown>) {
  proof ??= rootProbeProof;
  const result = { raw: snapshot.raw, display: snapshot.display, history: snapshot.history, options: snapshot.options, files: files.map(({ id: _, ...file }) => file), ...(proof ? { proof } : {}) };
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(JSON.stringify(result)));
  const fingerprint = [...new Uint8Array(digest)].map(value => value.toString(16).padStart(2, '0')).join('');
  await Promise.all([...pending]);
  if (failed) throw new Error('Incomplete benchmark trace');
  await invoke('benchmark_finish', { fingerprint, proof: proof ?? null });
}
