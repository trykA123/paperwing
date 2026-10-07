const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import { arrange, defaultIndex, isRisky, nameBonus, stepIndex } from './palette';

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

describe('mixed command and repository ranking', () => {
  const repo = (name: string) => ({ id: `repo:${name}`, label: `Open details: platform/${name}`, enabled: true, name });
  const command = (id: string, label: string) => ({ id, label, enabled: true, name: undefined as string | undefined });
  const scoring = {
    text: (c: { label: string; name?: string }) => c.name ?? c.label,
    bonus: (c: { name?: string }, q: string) => (c.name ? nameBonus(c.name, q) : 0),
  };
  const run = (query: string, list: ReturnType<typeof repo | typeof command>[]) => arrange(list, query, ['Actions', 'Repositories'], c => (c.id.startsWith('repo:') ? 'Repositories' : 'Actions'), scoring);

  test('"api" preselects the repository api over weakly matching commands', () => {
    const entries = run('api', [command('clone', 'Compare repository refs across set'), command('cleanup', 'Clean up merged branches'), repo('api-auth'), repo('api')]);
    expect(entries[defaultIndex(entries, isRisky)].command.id).toBe('repo:api');
  });

  test('"pu" never preselects Push', () => {
    const entries = run('pu', [command('push', 'Push 3 repositories'), command('pull', 'Pull 4 repositories (fast-forward)')]);
    expect(entries[0].command.id).toBe('push');
    expect(entries[defaultIndex(entries, isRisky)].command.id).toBe('pull');
  });

  test('nothing is preselected when only risky commands match', () => {
    const entries = run('push', [command('push', 'Push 3 repositories')]);
    expect(defaultIndex(entries, isRisky)).toBe(-1);
  });
});
