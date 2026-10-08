import { expect, test } from 'bun:test';
import { api } from './api';
import { withIpc } from './test-support/ipc-fixture';

test('progressive start forwards options and returns the generation identity', async () => {
  const options = { normalizeEol: true, ignoreWhitespace: false };
  await withIpc((command, args) => {
    expect(command).toBe('comparison_start');
    expect(args).toEqual({ id: 'session', options });
    return { id: 'session', generation: 3 };
  }, async () => {
    expect(await api.startComparison('session', options)).toEqual({ id: 'session', generation: 3 });
  });
});

test('progressive pages preserve pending phases and the pull cursor', async () => {
  const pending = { phase: 'pending', id: 'file', path: 'file', left: null, right: null, hint: 'changedId' };
  const response = { id: 'session', generation: 3, state: 'enriching', sequence: 501, rows: [pending], more: true,
    totals: null, history: null, snapshot: null, outcome: null, problem: null };
  await withIpc((command, args) => {
    expect(command).toBe('comparison_progress');
    expect(args).toEqual({ id: 'session', generation: 3, after: 500, limit: 17 });
    return response;
  }, async () => {
    expect(await api.comparisonProgress('session', 3, 500, 17)).toEqual(response);
  });
});

test('progressive pages default to bounded batches and preserve stale errors', async () => {
  const problem = { kind: 'staleGeneration', message: 'obsolete' };
  await withIpc((command, args) => {
    expect(command).toBe('comparison_progress');
    expect(args).toEqual({ id: 'session', generation: 2, after: 0, limit: 500 });
    throw problem;
  }, async () => {
    await expect(api.comparisonProgress('session', 2)).rejects.toEqual(problem);
  });
});
