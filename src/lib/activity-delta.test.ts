const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import type { Activity, ActivityDelta } from './api';
import { applyDelta, applyEntry, trimActivity } from './state/activity-delta';

const delta = (patch: Partial<ActivityDelta> = {}): ActivityDelta => ({
  id: 'git-1', context: 'Status', argv: ['status'], startedAt: 1, sequence: 0, elapsedMs: 0, state: 'running', exitCode: null,
  truncated: false, stdoutBytes: 0, stderrBytes: 0, from: 0, lines: [], ...patch,
});
const lines = (from: number, count: number) => Array.from({ length: count }, (_, index) => ({ sequence: from + index + 1, stream: 'stdout', text: `line ${from + index}` }));

describe('applyDelta', () => {
  test('a 500 line command split across deltas keeps every line in order', () => {
    let entries: ReadonlyMap<string, Activity> = new Map();
    for (const [from, count] of [[0, 0], [0, 180], [180, 220], [400, 100]]) {
      const result = applyDelta(entries, delta({ from, lines: lines(from, count), sequence: from + count }));
      expect(result.resync).toBe(false);
      entries = result.entries;
    }
    const output = entries.get('git-1')!.output;
    expect(output).toHaveLength(500);
    expect(output.map((line: { text: string }) => line.text)).toEqual(Array.from({ length: 500 }, (_, index) => `line ${index}`));
  });

  test('a delta that starts past the known lines asks for a snapshot and changes nothing', () => {
    const first = applyDelta(new Map(), delta({ from: 0, lines: lines(0, 2), sequence: 2 })).entries;
    const result = applyDelta(first, delta({ from: 5, lines: lines(5, 1), sequence: 6 }));
    expect(result.resync).toBe(true);
    expect(result.entries).toBe(first);
  });

  test('a repeated delta does not duplicate lines and an older sequence is ignored', () => {
    const first = applyDelta(new Map(), delta({ lines: lines(0, 3), sequence: 3 })).entries;
    const repeated = applyDelta(first, delta({ lines: lines(0, 3), sequence: 3 })).entries;
    expect(repeated.get('git-1')!.output).toHaveLength(3);
    const stale = applyDelta(repeated, delta({ from: 0, lines: lines(0, 1), sequence: 1, state: 'failed' }));
    expect(stale.entries).toBe(repeated);
  });

  test('the final delta updates the state and exit code without touching the lines', () => {
    const first = applyDelta(new Map(), delta({ lines: lines(0, 2), sequence: 2 })).entries;
    const done = applyDelta(first, delta({ from: 2, sequence: 3, state: 'completed', exitCode: 0 })).entries.get('git-1')!;
    expect(done.state).toBe('completed');
    expect(done.exitCode).toBe(0);
    expect(done.output).toHaveLength(2);
  });
});

describe('snapshots and trimming', () => {
  test('a full entry replaces an older sequence and ignores a stale one', () => {
    const entry = (sequence: number) => ({ ...delta({ sequence }), output: lines(0, sequence) }) as unknown as Activity;
    const first = applyEntry(new Map(), entry(2));
    expect(applyEntry(first, entry(1))).toBe(first);
    expect(applyEntry(first, entry(3)).get('git-1')!.output).toHaveLength(3);
  });

  test('trimming drops the oldest finished entries and keeps running ones', () => {
    const entries = new Map<string, Activity>();
    for (let index = 1; index <= 70; index += 1) {
      entries.set(`git-${index}`, { ...delta({ id: `git-${index}`, startedAt: index, state: index === 1 ? 'running' : 'completed' }), output: [] } as Activity);
    }
    const trimmed = trimActivity(entries);
    expect(trimmed.size).toBe(64);
    expect(trimmed.has('git-1')).toBe(true);
    expect(trimmed.has('git-2')).toBe(false);
    expect(trimmed.has('git-70')).toBe(true);
  });
});
