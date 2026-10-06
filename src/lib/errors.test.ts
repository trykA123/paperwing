const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import { describeError } from './errors';
import { plural } from './plural';

describe('describeError', () => {
  test('names the action, the cause and the fix', () => {
    expect(describeError('fatal: Authentication failed for https://x (401)', 'fetch the repository list')).toBe(
      "Couldn't fetch the repository list. The server rejected the credentials. Update the token in Settings > Sources.");
  });

  test('keeps only the first stderr line as detail for unknown failures', () => {
    const text = describeError('boom happened\nstack line 2\nstack line 3', 'push');
    expect(text).toContain('boom happened');
    expect(text).not.toContain('stack line');
  });

  test('handles empty input', () => {
    expect(describeError('', 'push')).toBe("Couldn't push. Try again.");
  });
});

describe('plural', () => {
  test('uses the singular only for one', () => {
    expect(plural(1, 'repository', 'repositories')).toBe('1 repository');
    expect(plural(0, 'repository', 'repositories')).toBe('0 repositories');
    expect(plural(2, 'file')).toBe('2 files');
  });
});
