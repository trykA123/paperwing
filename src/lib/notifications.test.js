import './test-support/svelte-loader.js';
import { beforeEach, expect, test } from 'bun:test';

const { NotificationStore, NOTICE_MS } = await import('./notifications.svelte.ts');

let now, queue;
const clock = {
    set: (run, ms) => { const handle = { run, at: now + ms }; queue.push(handle); return handle; },
    clear: handle => { queue = queue.filter(entry => entry !== handle); },
};
const advance = ms => {
    now += ms;
    for (const entry of queue.filter(item => item.at <= now)) { queue = queue.filter(item => item !== entry); entry.run(); }
};

beforeEach(() => { now = 0; queue = []; });

test('Success and info dismiss after four seconds, warnings after seven', () => {
    const store = new NotificationStore(clock);
    store.notify('Saved', 'success');
    store.notify('Heads up', 'info');
    store.notify('Careful', 'warn');
    advance(NOTICE_MS.success - 1);
    expect(store.items).toHaveLength(3);
    advance(1);
    expect(store.items.map(item => item.msg)).toEqual(['Careful']);
    advance(NOTICE_MS.warn - NOTICE_MS.success);
    expect(store.items).toHaveLength(0);
});

test('Errors and loading notices persist until dismissed', () => {
    const store = new NotificationStore(clock);
    const error = store.notify('Pull failed', 'error');
    const loading = store.notify('Fetching', 'loading');
    advance(10 * 60 * 1000);
    expect(store.items.map(item => item.id)).toEqual([error, loading]);
    store.dismiss(error);
    expect(store.items.map(item => item.id)).toEqual([loading]);
});

test('A repeated message re-arms the timer instead of stacking', () => {
    const store = new NotificationStore(clock);
    const first = store.notify('Saved', 'success');
    advance(3000);
    expect(store.notify('Saved', 'success')).toBe(first);
    advance(3000);
    expect(store.items).toHaveLength(1);
    advance(1000);
    expect(store.items).toHaveLength(0);
});

test('Hold pauses auto-dismiss and release restarts it', () => {
    const store = new NotificationStore(clock);
    const id = store.notify('Saved', 'success');
    store.hold(id, 'pointer');
    advance(60000);
    expect(store.items).toHaveLength(1);
    store.release(id, 'pointer');
    advance(NOTICE_MS.success);
    expect(store.items).toHaveLength(0);
});

test('Only the newest five stay and dropped notices lose their timers', () => {
    const store = new NotificationStore(clock);
    for (let index = 0; index < 7; index++) store.notify(`Message ${index}`, 'info');
    expect(store.items.map(item => item.msg)).toEqual(['Message 2', 'Message 3', 'Message 4', 'Message 5', 'Message 6']);
    expect(queue).toHaveLength(5);
});

test('Acting runs the handler and closes the notice', () => {
    const store = new NotificationStore(clock);
    const calls = [];
    const id = store.notify('Pull failed', 'error', { actions: [{ label: 'Retry', run: () => calls.push('retry') }, { label: 'Show log', run: () => calls.push('log') }] });
    store.act(id, store.items[0].actions[0]);
    expect(calls).toEqual(['retry']);
    expect(store.items).toHaveLength(0);
});

test('Updating a loading notice to success starts the success timer', () => {
    const store = new NotificationStore(clock);
    const id = store.notify('Fetching', 'loading');
    store.update(id, { msg: 'Fetched', kind: 'success' });
    expect(store.items[0].kind).toBe('success');
    advance(NOTICE_MS.success);
    expect(store.items).toHaveLength(0);
});

test('Eviction drops the oldest auto-dismissing notice and keeps errors and loading', () => {
    const store = new NotificationStore(clock);
    const error = store.notify('Pull failed', 'error');
    const loading = store.notify('Fetching', 'loading');
    for (let index = 0; index < 5; index++) store.notify(`Saved ${index}`, 'success');
    expect(store.items).toHaveLength(5);
    expect(store.items.map(item => item.id)).toContain(error);
    expect(store.items.map(item => item.id)).toContain(loading);
    expect(store.items.map(item => item.msg)).toEqual(['Pull failed', 'Fetching', 'Saved 2', 'Saved 3', 'Saved 4']);
    store.update(loading, { msg: 'Fetched', kind: 'success' });
    expect(store.items.find(item => item.id === loading).msg).toBe('Fetched');
});

test('Persistent notices are evicted oldest first only when nothing else remains', () => {
    const store = new NotificationStore(clock);
    for (let index = 0; index < 6; index++) store.notify(`Failed ${index}`, 'error');
    expect(store.items.map(item => item.msg)).toEqual(['Failed 1', 'Failed 2', 'Failed 3', 'Failed 4', 'Failed 5']);
});

test('Dismiss errors removes only errors', () => {
    const store = new NotificationStore(clock);
    store.notify('One', 'error');
    store.notify('Two', 'error');
    store.notify('Saved', 'success');
    store.dismissErrors();
    expect(store.items.map(item => item.msg)).toEqual(['Saved']);
});

test('Keyboard focus keeps a notice open after the pointer leaves', () => {
    const store = new NotificationStore(clock);
    const id = store.notify('Saved', 'success');
    store.hold(id, 'pointer');
    store.hold(id, 'focus');
    store.release(id, 'pointer');
    advance(60000);
    expect(store.items).toHaveLength(1);
    store.release(id, 'focus');
    advance(NOTICE_MS.success);
    expect(store.items).toHaveLength(0);
});

test('A repeated message does not restart the timer of a held notice', () => {
    const store = new NotificationStore(clock);
    const id = store.notify('Saved', 'success');
    store.hold(id, 'focus');
    store.notify('Saved', 'success');
    expect(queue).toHaveLength(0);
    advance(60000);
    expect(store.items).toHaveLength(1);
});

test('Dismissing clears hold state so a later notice starts clean', () => {
    const store = new NotificationStore(clock);
    const id = store.notify('Saved', 'success');
    store.hold(id, 'pointer');
    store.dismiss(id);
    store.release(id, 'pointer');
    store.notify('Saved', 'success');
    advance(NOTICE_MS.success);
    expect(store.items).toHaveLength(0);
});

test('A repeated message replaces detail and actions', () => {
    const store = new NotificationStore(clock);
    const id = store.notify('Pull failed', 'error', { detail: 'old', actions: [{ label: 'Retry', run() {} }] });
    store.notify('Pull failed', 'error', { detail: 'new', actions: [{ label: 'Show log', run() {} }] });
    expect(store.items).toHaveLength(1);
    expect(store.items[0].id).toBe(id);
    expect(store.items[0].detail).toBe('new');
    expect(store.items[0].actions.map(action => action.label)).toEqual(['Show log']);
});
