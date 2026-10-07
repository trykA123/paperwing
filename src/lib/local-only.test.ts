const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import { localOnlyNote } from './local-only';

describe('local-only note', () => {
  test('names the one host', () => expect(localOnlyNote(['github.com'])).toBe('Local only. Nothing on github.com changes.'));
  test('joins several hosts once each', () => expect(localOnlyNote(['github.com', 'git.acme.example', 'github.com'])).toBe('Local only. Nothing on github.com and git.acme.example changes.'));
  test('falls back to the remote', () => expect(localOnlyNote([])).toBe('Local only. Nothing on the remote changes.'));
});
