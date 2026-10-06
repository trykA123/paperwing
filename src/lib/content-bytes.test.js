import { expect, test } from 'bun:test';
import { api } from './api';
import { withIpc } from './test-support/ipc-fixture';

test('binary content transport preserves every byte and the requested identity', async () => {
  const bytes = Uint8Array.from({ length: 256 }, (_, index) => index);
  await withIpc((command, args) => {
    expect(command).toBe('comparison_content');
    expect(args).toEqual({ id: 'session', generation: 7, fileId: 'opaque', side: 'right' });
    return bytes.buffer;
  }, async () => {
    expect(await api.comparisonContent('session', 7, 'opaque', 'right', 'symlink')).toEqual({
      generation: 7, side: 'right', kind: 'symlink', bytes, binary: true,
    });
  });
});

test('binary content transport preserves BOM, CRLF and UTF-8 classification', async () => {
  for (const [bytes, binary] of [
    [Uint8Array.of(), false],
    [Uint8Array.of(239, 187, 191, 97, 13, 10), false],
    [Uint8Array.of(255), true],
    [Uint8Array.of(0), true],
  ]) {
    await withIpc(() => bytes.buffer, async () => {
      const content = await api.comparisonContent('session', 1, 'opaque', 'left', 'file');
      expect(content.bytes).toEqual(bytes);
      expect(content.binary).toBe(binary);
    });
  }
});

test('binary content transport rejects obsolete results and invalid payloads', async () => {
  await withIpc(() => { throw { kind: 'staleGeneration', message: 'obsolete' }; }, async () => {
    await expect(api.comparisonContent('session', 1, 'opaque', 'left', 'file')).rejects.toEqual({ kind: 'staleGeneration', message: 'obsolete' });
  });
  await withIpc(() => [1, 2], async () => {
    await expect(api.comparisonContent('session', 1, 'opaque', 'left', 'file')).rejects.toThrow('Invalid comparison content response');
  });
});
