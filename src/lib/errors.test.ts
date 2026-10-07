const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import { describeError, explainError, redact } from './errors';

const cause = (raw: string) => explainError(raw).cause;

describe('describeError', () => {
  test('names the action, the cause, the fix and the detail', () => {
    expect(describeError('fatal: Authentication failed for https://x/y.git', 'fetch the repository list')).toBe(
      "Couldn't fetch the repository list. The server rejected the token. Update it in Settings > Sources. Details: Authentication failed for https://x/y.git");
  });
  test('always appends details, capped at 160 characters', () => {
    const text = describeError('x'.repeat(400), 'push');
    expect(text.split('Details: ')[1].length).toBe(160);
  });
  test('uses only the first line of stderr', () => {
    const text = describeError('boom\nstack line 2', 'push');
    expect(text).toContain('Details: boom');
    expect(text).not.toContain('stack line');
  });
  test('handles empty input', () => {
    expect(describeError('', 'push')).toBe("Couldn't push. Try again.");
  });
});

describe('cause rules', () => {
  test('Git missing', () => {
    expect(cause('sh: git: command not found')).toBe('Git is not installed or not on PATH. Install Git or add it to PATH, then restart Skein.');
    expect(cause("'git' is not recognized as an internal or external command")).toContain('not on PATH');
  });
  test('GitHub 403 rate limit, SSO and bad token are distinct', () => {
    expect(cause('HTTP 403: API rate limit exceeded for user')).toContain('API rate limit');
    expect(cause('HTTP 403: Resource protected by organization SAML enforcement')).toContain('authorize the token for SSO');
    expect(cause('HTTP 403: Forbidden')).toContain('rejected the token');
    expect(cause('HTTP 401 Bad credentials')).toContain('rejected the token');
  });
  test('not found, network, folder permission', () => {
    expect(cause('HTTP 404: Not Found')).toContain('could not find');
    expect(cause('Could not resolve host: git.acme.example')).toContain('did not answer');
    expect(cause('EACCES: permission denied, mkdir')).toContain('not writable');
  });
  test('lock matches whole words only', () => {
    expect(cause("Unable to create '.git/index.lock': File exists")).toContain('Another Git process');
    expect(cause('request blocked by proxy')).toBeNull();
    expect(cause('clock skew detected')).toBeNull();
  });
  test('not a repository', () => {
    expect(cause('fatal: not a git repository')).toContain('not a repository');
  });
});

describe('redact', () => {
  test('hides URL credentials and tokens in the detail', () => {
    expect(redact('clone https://user:s3cret@host/x.git failed')).toBe('clone https://***@host/x.git failed');
    expect(redact('Authorization: Bearer abcdef123456')).toBe('Authorization: Bearer ***');
    expect(redact('bad ghp_abcdefghij1234')).toBe('bad ***');
    expect(describeError('fatal: https://u:p4ss@h/r.git 401', 'fetch')).not.toContain('p4ss');
  });
});
