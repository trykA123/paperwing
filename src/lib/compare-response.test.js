import { describe, expect, test } from 'bun:test';
import { api } from './api';
import { readComparisonContent } from './content-bytes';
import { withIpc } from './test-support/ipc-fixture';
import { formatCompareProblem } from './compare-response';

describe('GitHub comparison problems', () => {
  test('shows a rate limit in the owner local time', () => {
    const retryAt = Date.UTC(2026, 9, 8, 10, 30);
    const problem = { kind: 'githubRateLimited', side: null, message: 'limited', retryAt };
    const time = new Date(retryAt).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', hour12: false });
    expect(formatCompareProblem(problem).message).toBe(`GitHub rate limit, try again at ${time}`);
    expect(problem.message).toBe('limited');
  });

  test('retains unavailable and invalid reset messages', () => {
    for (const problem of [
      { kind: 'githubNotFound', side: null, message: 'missing' },
      { kind: 'githubRateLimited', side: null, message: 'limited' },
      { kind: 'githubRateLimited', side: null, message: 'limited', retryAt: Number.NaN },
    ]) expect(formatCompareProblem(problem)).toBe(problem);
  });

});

test('comparison wrapper uses explicit source without changing the default request', async () => {
  const calls = [];
  await withIpc((command, args) => {
    calls.push({ command, args });
    return { status: 'ready', snapshot: { source: 'github', truncated: { files: true, commits: false } } };
  }, async () => {
    const options = { normalizeEol: false, ignoreWhitespace: false };
    expect((await api.refreshComparison('comparison-1', options)).snapshot.source).toBe('github');
    expect((await api.refreshComparison('comparison-1', options, 'github')).snapshot.truncated.files).toBe(true);
    expect(calls).toEqual([
      { command: 'comparison_refresh', args: { id: 'comparison-1', options } },
      { command: 'comparison_refresh', args: { id: 'comparison-1', options, source: 'github' } },
    ]);
  });
});

test('comparison wrapper formats returned rate limits in local time', async () => {
  const problem = { kind: 'githubRateLimited', side: null, message: 'limited', retryAt: 1791455400000 };
  await withIpc(() => ({ status: 'unavailable', problem }), async () => {
    expect((await api.refreshComparison('comparison-1', { normalizeEol: false, ignoreWhitespace: false })).problem).toEqual(formatCompareProblem(problem));
  });
});




test('blob reads format rate limits and preserve other rejected problems', async () => {
  const request = { id: 'comparison-1', generation: 1, fileId: 'file-1', side: 'right', kind: 'file' };
  for (const problem of [
    { kind: 'githubRateLimited', message: 'limited', side: null, retryAt: 1791455400000 },
    { kind: 'githubNotFound', message: 'missing', side: null },
  ]) {
    await withIpc(() => { throw problem; }, async () => {
      await expect(readComparisonContent(request)).rejects.toEqual(formatCompareProblem(problem));
    });
  }
});
