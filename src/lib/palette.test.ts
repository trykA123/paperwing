const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import { arrange, defaultIndex, stepIndex } from './palette';

const item = (id: string, enabled = true) => ({ id, label: id, enabled });
const groups = ['Actions', 'Navigate'];
const groupOf = (c: { id: string }) => (c.id.startsWith('go') ? 'Navigate' : 'Actions');

describe('palette order', () => {
  test('puts actions before navigation and keeps source order inside a group', () => {
    const out = arrange([item('go-a'), item('fetch'), item('go-b'), item('pull')], '', groups, groupOf).map(e => e.command.id);
    expect(out).toEqual(['fetch', 'pull', 'go-a', 'go-b']);
  });

  test('never preselects a risky command while a safe one is enabled', () => {
    const entries = arrange([item('close'), item('fetch')], '', groups, () => 'Actions');
    expect(entries[defaultIndex(entries, c => c.id === 'close')].command.id).toBe('fetch');
  });

  test('skips disabled entries and wraps', () => {
    const entries = arrange([item('a'), item('b', false), item('c')], '', groups, () => 'Actions');
    expect(stepIndex(entries, 0, 1)).toBe(2);
    expect(stepIndex(entries, 2, 1)).toBe(0);
    expect(stepIndex(entries, 0, -1)).toBe(2);
  });

  test('ranks by match when typing', () => {
    const out = arrange([item('use a better theme'), item('settings')], 'set', groups, () => 'Actions').map(e => e.command.id);
    expect(out[0]).toBe('settings');
  });
});
