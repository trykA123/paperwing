import './test-support/svelte-loader.js';
import { describe, expect, test } from 'bun:test';
import { withIpc } from './test-support/ipc-fixture.js';
import { compareTagsDesc, createTags, deleteLocalTags, deleteRemoteTags, moveConfirmMessage, planTags, refreshAfterTagChange, remoteDeleteMessage } from './tags-set.ts';

const target = name => ({ path: `/r/${name}`, name, remote: 'origin', commit: null });
const repo = path => path.split('/').pop();
const old = { name: 'v1', commit: 'c'.repeat(40), object: 'd'.repeat(40), annotated: true, subject: 'old' };

function fake({ tags = {}, fail = {}, pushFail = {}, heads = {} } = {}) {
  const calls = [];
  return {
    calls,
    localStatus: async paths => paths.map(path => ({ path, sha: heads[repo(path)] ?? '' })),
    listTags: async path => { if (fail[repo(path)] === 'list') throw 'index.lock exists'; return tags[repo(path)] ?? []; },
    createTag: async (path, request) => {
      calls.push(['create', repo(path), request]);
      if (fail[repo(path)] === 'create') throw 'index.lock exists';
      const existing = (tags[repo(path)] ?? []).find(tag => tag.name === request.name);
      if (existing && !request.moveExisting) throw `A tag named ${request.name} already exists`;
      return { name: request.name, commit: 'e'.repeat(40), object: 'f'.repeat(40), annotated: !!request.message, previousObject: existing?.object ?? null };
    },
    pushTag: async (path, remote, name, lease) => {
      calls.push(['push', repo(path), remote, lease]);
      const rule = pushFail[repo(path)];
      if (rule === 'always' || (rule === 'lease' && lease)) throw 'rejected: stale info';
      return { remote, name, forced: lease !== null };
    },
    deleteTag: async (path, name) => { calls.push(['delete', repo(path)]); if (fail[repo(path)] === 'delete') throw 'There is no local tag named ' + name; return { name, object: 'f'.repeat(40) }; },
    deleteRemoteTag: async (path, remote, name, expected) => { calls.push(['delete-remote', repo(path), remote, expected]); if (fail[repo(path)] === 'remote') throw 'could not resolve host'; return { remote, name, forced: false }; },
  };
}
const request = (patch = {}) => ({ name: 'v2.4.0', message: 'Release', push: false, move: false, ...patch });

describe('plan', () => {
  test('marks existing tags and keeps a failed read on its row', async () => {
    const api = fake({ tags: { b: [{ ...old, name: 'v2.4.0' }] }, fail: { c: 'list' } });
    const rows = await planTags([target('a'), target('b'), target('c')], 'v2.4.0', api);
    expect(rows.map(row => !!row.existing)).toEqual([false, true, false]);
    expect(rows[2].error).toContain('Another Git process');
  });
});

describe('create', () => {
  test('one failure among three does not stop the others and keeps order', async () => {
    const api = fake({ fail: { b: 'create' } });
    const rows = await planTags([target('a'), target('b'), target('c')], 'v2.4.0', api);
    const seen = [];
    const result = await createTags(rows, request({ push: true }), api, index => seen.push(index));
    expect(result.map(row => row.status)).toEqual(['pushed', 'failed', 'pushed']);
    expect(result[1].error).toContain('Another Git process');
    expect(seen).toEqual([0, 1, 2]);
    expect(api.calls.filter(call => call[0] === 'push').map(call => call[1])).toEqual(['a', 'c']);
  });

  test('a duplicate name is refused without calling create', async () => {
    const api = fake({ tags: { a: [{ ...old, name: 'v2.4.0' }] } });
    const rows = await planTags([target('a'), target('b')], 'v2.4.0', api);
    const result = await createTags(rows, request(), api);
    expect(result.map(row => row.status)).toEqual(['refused', 'created']);
    expect(result[0].error).toContain('already exists');
    expect(api.calls.filter(call => call[0] === 'create').map(call => call[1])).toEqual(['b']);
  });

  test('the backend refusal is reported when the plan was stale', async () => {
    const api = fake({ tags: { a: [{ ...old, name: 'v2.4.0' }] } });
    const result = await createTags([{ ...target('a'), existing: null, error: null }], request(), api);
    expect(result[0].status).toBe('failed');
    expect(result[0].error).toContain('already exists');
  });

  test('a blank message makes a lightweight tag; push is skipped unless asked', async () => {
    const api = fake();
    const rows = await planTags([target('a')], 'v2.4.0', api);
    const [row] = await createTags(rows, request({ message: '  ' }), api);
    expect(api.calls).toEqual([['create', 'a', { name: 'v2.4.0', message: null, target: null, moveExisting: false }]]);
    expect(row.status).toBe('created');
  });

  test('a push failure keeps the created tag', async () => {
    const api = fake({ pushFail: { a: 'always' } });
    const rows = await planTags([target('a')], 'v2.4.0', api);
    const [row] = await createTags(rows, request({ push: true }), api);
    expect(row.status).toBe('push-failed');
    expect(row.created).not.toBeNull();
    expect(row.error).toContain('Couldn');
  });

  test('moving needs the flag and pushes with a lease on the old object', async () => {
    const api = fake({ tags: { a: [{ ...old, name: 'v2.4.0' }] } });
    const rows = await planTags([target('a')], 'v2.4.0', api);
    const [row] = await createTags(rows, request({ move: true, push: true }), api);
    expect(api.calls[0][2].moveExisting).toBe(true);
    expect(api.calls[1]).toEqual(['push', 'a', 'origin', old.object]);
    expect(row.status).toBe('pushed');
  });

  test('a move falls back to an absent-remote lease and reports the first error when both fail', async () => {
    const lease = fake({ tags: { a: [{ ...old, name: 'v2.4.0' }] }, pushFail: { a: 'lease' } });
    const rows = await planTags([target('a')], 'v2.4.0', lease);
    await createTags(rows, request({ move: true, push: true }), lease);
    expect(lease.calls.filter(call => call[0] === 'push').map(call => call[3])).toEqual([old.object, '']);
    const never = fake({ tags: { a: [{ ...old, name: 'v2.4.0' }] }, pushFail: { a: 'always' } });
    const [row] = await createTags(await planTags([target('a')], 'v2.4.0', never), request({ move: true, push: true }), never);
    expect(row.status).toBe('push-failed');
  });

  test('planning pins each target to the commit HEAD is at now', async () => {
    const api = fake({ heads: { a: '1'.repeat(40) } });
    const rows = await planTags([target('a'), target('b'), { ...target('c'), commit: '9'.repeat(40) }], 'v2.4.0', api);
    expect(rows.map(row => row.commit)).toEqual(['1'.repeat(40), null, '9'.repeat(40)]);
    await createTags(rows.slice(0, 1), request(), api);
    expect(api.calls[0][2].target).toBe('1'.repeat(40));
  });

  test('the move confirmation shows old and new commit', () => {
    const text = moveConfirmMessage([{ ...target('a'), commit: '1'.repeat(40), existing: old, error: null }, { ...target('b'), existing: old, error: null }], 'v1');
    expect(text).toContain('a: cccccccc to 11111111');
    expect(text).toContain('b: cccccccc to HEAD');
  });
});

describe('order', () => {
  test('versions sort newest first, v10 above v9, a pre-release below its release', () => {
    const names = ['v9', 'v10', 'v2.5.0-rc', 'v2.5.0', 'v2.10.0', 'v2.9.1', 'nightly', 'release-b', 'release-a'];
    expect([...names].sort(compareTagsDesc)).toEqual(['v10', 'v9', 'v2.10.0', 'v2.9.1', 'v2.5.0', 'v2.5.0-rc', 'release-b', 'release-a', 'nightly']);
  });
});

describe('delete', () => {
  test('local delete records a failure and goes on', async () => {
    const api = fake({ fail: { b: 'delete' } });
    const rows = (await planTags([target('a'), target('b'), target('c')], 'v1', fake())).map(row => ({ ...row, existing: old }));
    const result = await deleteLocalTags(rows, 'v1', api);
    expect(result.map(row => row.status)).toEqual(['removed', 'failed', 'removed']);
  });

  test('remote delete is separate, per remote, and names every repository', async () => {
    const api = fake({ fail: { b: 'remote' } });
    const targets = [{ ...target('a'), expected: old.object }, { ...target('b'), remote: 'upstream', expected: old.object }, { ...target('c'), expected: old.object }];
    const result = await deleteRemoteTags(targets, 'v1', api);
    expect(result.map(row => row.status)).toEqual(['removed', 'failed', 'removed']);
    expect(api.calls.map(call => call[2])).toEqual(['origin', 'upstream', 'origin']);
    expect(api.calls.every(call => call[0] === 'delete-remote')).toBe(true);
    const text = remoteDeleteMessage(targets, 'v1');
    expect(text).toContain('a (origin)');
    expect(text).toContain('b (upstream): object dddddddd');
  });

  test('a repository with no known object is listed as refused and never submitted', async () => {
    const api = fake();
    const targets = [{ ...target('a'), expected: old.object }, { ...target('b'), expected: null }];
    const result = await deleteRemoteTags(targets, 'v1', api);
    expect(result.map(row => row.status)).toEqual(['removed', 'refused']);
    expect(api.calls.map(call => call[1])).toEqual(['a']);
    expect(api.calls[0][3]).toBe(old.object);
    expect(remoteDeleteMessage(targets, 'v1')).toContain('b (origin): unknown — will be refused');
  });
});

describe('ref epoch', () => {
  test('refresh forces ref lists and trees for every changed repository once', () => {
    const calls = [];
    refreshAfterTagChange(['/r/a', '/r/b', '/r/a'], {
      urlsFor: path => [`https://x.test/${repo(path)}.git`, 'https://x.test/shared.git'],
      ensureRefs: (urls, force) => calls.push(['refs', urls, force]),
      loadTree: (path, force) => calls.push(['tree', path, force]),
    });
    expect(calls).toEqual([
      ['refs', ['https://x.test/a.git', 'https://x.test/shared.git', 'https://x.test/b.git'], true],
      ['tree', '/r/a', true], ['tree', '/r/b', true],
    ]);
  });

  test('a forced refresh replaces the cached tag list and bumps the epoch', async () => {
    const { app } = await import('./state.svelte.ts');
    const url = 'https://x.test/a.git';
    const seen = [];
    await withIpc(command => {
      seen.push(command);
      return Promise.resolve([{ url, branches: ['main'], tags: ['v2.4.0'], branchShas: ['a'], tagShas: ['b'], error: null }]);
    }, async () => {
      const state = new app.constructor();
      state.refs[url] = { branches: ['main'], tags: ['old'] };
      await state.ensureRefs([url], true);
      await state.ensureRefs([url], false);
      expect(state.refs[url].tags).toEqual(['v2.4.0']);
      expect(seen.filter(command => command === 'get_refs_many').length).toBe(1);
    });
  });
});
