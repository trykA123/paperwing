import { expect, test } from 'bun:test';
import { fileSignature } from './file-signature';

const entry = size => ({ kind: 'file', size, modifiedMs: 5, reason: null, source: 'workingTree' });
const file = (size = 10) => ({ id: 'f1', path: 'a.c', left: entry(size), right: null, displayStatus: 'different' });

test('a refetched file list entry with the same content has the same signature', () => {
    expect(fileSignature(file())).toBe(fileSignature({ ...file(), displayStatus: 'different' }));
    expect(fileSignature(file())).not.toBe('');
});

test('a changed or missing entry has a different signature', () => {
    expect(fileSignature(file(11))).not.toBe(fileSignature(file(10)));
    expect(fileSignature(undefined)).toBe('');
});
