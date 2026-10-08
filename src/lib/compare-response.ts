import { invoke } from '@tauri-apps/api/core';
import type { CompareOptions, CompareProblem, CompareResult, CompareSource } from './api';

export function formatCompareProblem(problem: CompareProblem): CompareProblem {
  if (problem.kind !== 'githubRateLimited' || typeof problem.retryAt !== 'number') return problem;
  const date = new Date(problem.retryAt);
  if (!Number.isFinite(date.getTime())) return problem;
  const time = date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', hour12: false });
  return { ...problem, message: `GitHub rate limit, try again at ${time}` };
}

export async function refreshComparison(id: string, options: CompareOptions, source?: CompareSource): Promise<CompareResult> {
  const result = await invokeComparison<CompareResult>('comparison_refresh', { id, options, ...(source === undefined ? {} : { source }) });
  return result.status === 'ready' ? result : { ...result, problem: formatCompareProblem(result.problem) };
}

export async function invokeComparison<T>(command: string, args: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    if (typeof error === 'object' && error !== null && 'kind' in error && 'message' in error && 'side' in error
      && typeof error.kind === 'string' && typeof error.message === 'string'
      && (error.side === null || typeof error.side === 'string')) {
      throw formatCompareProblem(error as CompareProblem);
    }
    throw error;
  }
}
