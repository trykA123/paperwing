import { describe, expect, test } from 'bun:test';
import { restoreStash, rowStatus, stashedRows, switchWithStash } from './stash-switch.ts';

const target = name => ({ path: `/r/${name}`, name, branch: 'release' });
const fake = outcomes => {
  const calls = [];
  return { calls, switchWithStash: async (path, branch) => {
    calls.push([path, branch]);
    const outcome = outcomes[path.split('/').pop()];
    if (outcome instanceof Error) throw outcome.message;
    return outcome;
  } };
};

describe('set-wide switch', () => {
  test('clean repository switches without a stash', async () => {
    const api = fake({ a: { stashed: null, switched: true, error: null } });
    const [row] = await switchWithStash([target('a')], api);
    expect(api.calls).toEqual([['/r/a', 'release']]);
    expect(rowStatus(row)).toBe('switched');
  });

  test('dirty repository reports the stash oid', async () => {
    const api = fake({ a: { stashed: 'abc123', switched: true, error: null } });
    const [row] = await switchWithStash([target('a')], api);
    expect(row.stashed).toBe('abc123');
    expect(rowStatus(row)).toBe('switched-stashed');
  });

  test('switch failure keeps the stash visible', async () => {
    const api = fake({ a: { stashed: 'abc123', switched: false, error: 'There is no branch named release' } });
    const [row] = await switchWithStash([target('a')], api);
    expect(rowStatus(row)).toBe('failed-stashed');
    expect(row.error).toContain('no branch');
  });

  test('one failure among three does not stop the rest and runs in order', async () => {
    const api = fake({ a: { stashed: 'o1', switched: true, error: null }, b: new Error('index.lock exists'), c: { stashed: 'o3', switched: true, error: null } });
    const seen = [];
    const rows = await switchWithStash([target('a'), target('b'), target('c')], api, index => seen.push(index));
    expect(api.calls.map(call => call[0])).toEqual(['/r/a', '/r/b', '/r/c']);
    expect(seen).toEqual([0, 1, 2]);
    expect(rows.map(rowStatus)).toEqual(['switched-stashed', 'failed', 'switched-stashed']);
    expect(rows[1].error).toContain('Another Git process');
    expect(stashedRows(rows).map(row => row.stashed)).toEqual(['o1', 'o3']);
  });
});

describe('restore', () => {
  const api = outcome => ({ stashApply: async () => { if (outcome instanceof Error) throw outcome.message; return outcome; } });

  test('clean apply offers drop', async () => {
    const state = await restoreStash('/r/a', 'o1', api({ applied: true, stashKept: true, indexRestored: true, conflicted: [], error: null }));
    expect(state).toEqual({ phase: 'applied', kept: true, indexRestored: true });
  });

  test('conflict lists files and never claims applied', async () => {
    const state = await restoreStash('/r/a', 'o1', api({ applied: false, stashKept: true, indexRestored: true, conflicted: ['a.txt'], error: 'CONFLICT' }));
    expect(state).toEqual({ phase: 'conflicted', files: ['a.txt'], message: 'CONFLICT' });
  });

  test('thrown errors become a failed state', async () => {
    const state = await restoreStash('/r/a', 'o1', api(new Error('Resolve the current conflicts first')));
    expect(state.phase).toBe('failed');
  });
});
