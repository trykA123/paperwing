import './test-support/svelte-loader.js';
import { beforeEach, expect, test } from 'bun:test';

const { DiagnosticsStore, formatByteSize, progressPercent } = await import('./diagnostics.svelte.ts');

let notices, calls, emit, previewResult, saveResult;
const transport = {
  status: async () => { calls.push('status'); if (transport.absent) throw 'command diagnostics_status not found'; return { sampling: true, samples: 42 }; },
  preview: async () => { calls.push('preview'); emit?.({ phase: 'scale', completed: 1, total: 3 }); return previewResult(); },
  cancel: async () => { calls.push('cancel'); return true; },
  save: async () => { calls.push('save'); return saveResult(); },
  onProgress: async handler => { emit = handler; return () => { emit = null; }; },
};
const make = () => new DiagnosticsStore(transport, (message, kind) => notices.push([message, kind]));

beforeEach(() => {
  notices = []; calls = []; emit = null; transport.absent = false;
  previewResult = async () => '{"version":1}'; saveResult = async () => true;
});

test('section stays hidden when the status command is absent', async () => {
  transport.absent = true;
  const store = make();
  await store.probe();
  expect(store.available).toBe(false);
  expect(notices).toEqual([]);
});

test('probe shows the sample count when the command exists', async () => {
  const store = make();
  await store.probe();
  expect(store.available).toBe(true);
  expect(store.samples).toBe(42);
});

test('generate stores the exact preview and its byte size', async () => {
  previewResult = async () => '{"name":"é"}';
  const store = make();
  await store.probe();
  await store.generate();
  expect(store.phase).toBe('ready');
  expect(store.preview).toBe('{"name":"é"}');
  expect(store.bytes).toBe(13);
});

test('a refused preview shows the backend message and keeps no preview', async () => {
  previewResult = async () => { throw 'Diagnostics contained identifying text; nothing was written'; };
  const store = make();
  await store.probe();
  await store.generate();
  expect(store.phase).toBe('idle');
  expect(store.error).toBe('Diagnostics contained identifying text; nothing was written');
  expect(store.preview).toBe('');
  expect(calls.filter(call => call === 'preview')).toHaveLength(1);
});

test('cancelling is not an error', async () => {
  let release;
  previewResult = () => new Promise((_, reject) => { release = () => reject('Diagnostics collection cancelled'); });
  const store = make();
  await store.probe();
  const running = store.generate();
  await Promise.resolve();
  await store.cancel();
  release();
  await running;
  expect(calls).toContain('cancel');
  expect(store.phase).toBe('idle');
  expect(store.error).toBe('');
});

test('save toasts once and a dismissed dialog stays silent', async () => {
  const store = make();
  await store.probe();
  await store.generate();
  await store.save();
  expect(notices).toEqual([['Diagnostics saved', 'success']]);
  saveResult = async () => false;
  await store.save();
  expect(notices).toHaveLength(1);
  expect(store.phase).toBe('ready');
});

test('a refused save clears the preview and shows the message', async () => {
  saveResult = async () => { throw 'Diagnostics contained identifying text; nothing was written'; };
  const store = make();
  await store.probe();
  await store.generate();
  await store.save();
  expect(store.error).toContain('identifying text');
  expect(store.preview).toBe('');
  expect(store.phase).toBe('idle');
});

test('formatting helpers', () => {
  expect(formatByteSize(512)).toBe('512 B');
  expect(formatByteSize(2048)).toBe('2.0 KB');
  expect(formatByteSize(3 * 1024 * 1024)).toBe('3.0 MB');
  expect(progressPercent({ phase: 'scale', completed: 1, total: 4 })).toBe(25);
  expect(progressPercent(null)).toBe(0);
});
