import { expect, test } from 'bun:test';
import { ForegroundRequests } from './state/foreground-requests';
import { deferred } from './test-support/ipc-fixture.js';

function request(response, overrides = {}) {
  return { produce: () => response.promise, publish() {}, fail() {}, settled() {}, ...overrides };
}

test('two consumers share one producer and cancellation preserves the remaining consumer', async () => {
  const requests = new ForegroundRequests(), response = deferred(), consumer = new AbortController();
  let produced = 0;
  const published = [];
  const options = request(response, { produce() { produced++; return response.promise; }, publish: value => published.push(value) });
  const first = requests.run('metadata', { ...options, signal: consumer.signal });
  const second = requests.run('metadata', options);
  consumer.abort();
  await first;
  expect(requests.size).toBe(1);
  expect(produced).toBe(1);
  response.resolve('current');
  await second;
  expect(published).toEqual(['current']);
  expect(requests.size).toBe(0);
});

test('abandoned work retains producer admission until completion and cannot publish', async () => {
  const requests = new ForegroundRequests({ producers: 1, consumers: 2, queued: 0 }), response = deferred();
  const consumer = new AbortController(), published = [], failures = [];
  const pending = requests.run('first', request(response, { signal: consumer.signal, publish: value => published.push(value) }));
  consumer.abort();
  await pending;
  expect(requests.size).toBe(1);
  await requests.run('second', request(deferred(), { fail: error => failures.push(String(error)) }));
  expect(failures).toHaveLength(1);
  response.resolve('obsolete');
  await response.promise;
  expect(published).toEqual([]);
  expect(requests.size).toBe(0);
});

test('consumer limits reject excess waiters and restore admission after cancellation', async () => {
  const requests = new ForegroundRequests({ producers: 2, consumers: 1, queued: 0 }), response = deferred();
  const controller = new AbortController(), failures = [];
  const first = requests.run('shared', request(response, { signal: controller.signal }));
  await requests.run('shared', request(response, { fail: error => failures.push(String(error)) }));
  expect(failures).toHaveLength(1);
  controller.abort();
  await first;
  const next = requests.run('shared', request(response));
  response.resolve('accepted');
  await next;
  expect(requests.size).toBe(0);
});

test('failed producers are retryable without retained flights after an error flood', async () => {
  const requests = new ForegroundRequests();
  for (let index = 0; index < 500; index++) {
    const response = deferred(), failures = [];
    const waiting = requests.run('retry', request(response, { fail: error => failures.push(String(error)) }));
    response.reject(new Error('fixture failure'));
    await waiting;
    expect(failures).toHaveLength(1);
    expect(requests.size).toBe(0);
  }
  const response = deferred(), published = [];
  const retry = requests.run('retry', request(response, { publish: value => published.push(value) }));
  response.resolve('recovered');
  await retry;
  expect(published).toEqual(['recovered']);
});

test('an already cancelled consumer never starts a producer', async () => {
  const requests = new ForegroundRequests(), consumer = new AbortController();
  consumer.abort();
  await requests.run('cancelled', request(deferred(), { signal: consumer.signal, produce() { throw new Error('must not run'); } }));
  expect(requests.size).toBe(0);
});

const settle = () => new Promise(resolve => setImmediate(resolve));

test('100 keys all resolve with 32 producers, queued first in first out', async () => {
  const requests = new ForegroundRequests({ producers: 32, consumers: 256, queued: 1024 });
  const responses = [], started = [], published = [], failures = [];
  const waits = Array.from({ length: 100 }, (_, index) => {
    const response = deferred();
    responses.push(response);
    return requests.run(`key-${index}`, request(response, {
      produce() { started.push(index); return response.promise; },
      publish: value => published.push(value), fail: error => failures.push(String(error)),
    }));
  });
  expect(started).toHaveLength(32);
  for (let index = 0; index < 100; index++) {
    responses[index].resolve(index);
    await settle();
  }
  await Promise.all(waits);
  expect(started).toEqual(Array.from({ length: 100 }, (_, index) => index));
  expect(published).toHaveLength(100);
  expect(failures).toEqual([]);
  expect(requests.size).toBe(0);
});

test('queued foreground work starts before queued background work', async () => {
  const requests = new ForegroundRequests({ producers: 1, consumers: 16, queued: 8 });
  const running = deferred(), started = [];
  const queued = (key, priority) => requests.run(key, request(deferred(), { priority, produce() { started.push(key); return new Promise(() => {}); } }));
  void requests.run('running', request(running));
  void queued('background', 'background');
  void queued('foreground', 'foreground');
  running.resolve('done');
  await settle();
  expect(started).toEqual(['foreground']);
});

test('requests fail only past the queue bound', async () => {
  const requests = new ForegroundRequests({ producers: 1, consumers: 16, queued: 2 }), failures = [];
  for (const key of ['a', 'b', 'c', 'd']) void requests.run(key, request(deferred(), { fail: error => failures.push([key, String(error)]) }));
  expect(failures).toEqual([['d', 'Error: Metadata requests are busy; retry after a current request finishes']]);
  expect(requests.size).toBe(3);
});

test('a queued request whose consumers all left never starts and frees its slot', async () => {
  const requests = new ForegroundRequests({ producers: 1, consumers: 16, queued: 1 });
  const running = deferred(), consumer = new AbortController(), settled = [];
  void requests.run('running', request(running));
  const waiting = requests.run('queued', request(deferred(), { signal: consumer.signal, settled: () => settled.push('queued'), produce() { throw new Error('must not run'); } }));
  consumer.abort();
  await waiting;
  expect(settled).toEqual(['queued']);
  expect(requests.size).toBe(1);
  const failures = [];
  void requests.run('next', request(deferred(), { fail: error => failures.push(error) }));
  expect(failures).toEqual([]);
  running.resolve('done');
  await settle();
});
