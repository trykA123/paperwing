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
  const requests = new ForegroundRequests({ producers: 1, consumers: 2 }), response = deferred();
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
  const requests = new ForegroundRequests({ producers: 2, consumers: 1 }), response = deferred();
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
