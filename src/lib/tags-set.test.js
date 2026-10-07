import './test-support/svelte-loader.js';
import { describe, expect, test } from 'bun:test';
import { withIpc } from './test-support/ipc-fixture.js';
import { compareTagsDesc, createGithubReleases, pendingReleaseTargets, createTags, deleteLocalTags, moveConfirmMessage, planTags, refreshAfterTagChange, releaseTargets } from './tags-set.ts';

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

describe('GitHub releases', () => {
  const pushed = (name, patch = {}) => ({ ...target(name), status: 'pushed', created: { name: 'v1', annotated: true }, pushed: { remote: 'origin', name: 'v1' }, error: null, ...patch });

  test('only a successfully pushed annotated tag is eligible', () => {
    const rows = [pushed('a'), pushed('b', { status: 'created', pushed: null }), pushed('c', { status: 'push-failed', pushed: null }), pushed('d', { created: { annotated: false } }), pushed('e', { status: 'failed', created: null }), pushed('f', { pushed: null })];
    expect(releaseTargets(rows).map(row => row.name)).toEqual(['a']);
  });

  test('missing draft defaults to true and one denied repository does not stop the rest', async () => {
    const calls = [], seen = [];
    const api = { createGithubRelease: async (path, tag, notes, draft) => {
      calls.push([path, tag, notes, draft]);
      if (repo(path) === 'b') throw { kind: 'message', message: 'GitHub access denied; check token repository permissions' };
      return { id: 33, url: `https://gitint.company.com/admin/${repo(path)}/releases/33`, draft };
    } };
    const results = await createGithubReleases(['a', 'b', 'c'].map(name => pushed(name)), { tag: 'v1', notes: 'Tag message' }, api, row => seen.push(row.name));
    expect(results.map(row => row.status)).toEqual(['created', 'failed', 'created']);
    expect(results[1].error).toContain('permissions');
    expect(results[2].release.url).toContain('gitint.company.com');
    expect(calls.every(call => call[2] === 'Tag message' && call[3] === true)).toBe(true);
    expect(seen).toEqual(['a', 'b', 'c']);
  });

  test('release creation uses each row push remote', async () => {
    const calls = [];
    const api = { createGithubRelease: async (...args) => { calls.push(args); return { id: 33, url: 'https://github.com/admin/a/releases/33', draft: true }; } };
    await createGithubReleases([pushed('a'), pushed('b', { remote: 'upstream' })], { tag: 'v1', notes: '' }, api);
    expect(calls.map(call => call[4])).toEqual(['origin', 'upstream']);
  });

  test('retries select failed rows and preserve successes and refusals', () => {
    const rows = ['a', 'b', 'c', 'd'].map(name => pushed(name));
    const results = [
      { ...target('a'), status: 'created', release: { id: 33 }, error: null },
      { ...target('b'), status: 'failed', release: null, error: 'permissions' },
      { ...target('c'), status: 'refused', release: null, error: 'Push first' },
    ];
    expect(pendingReleaseTargets(rows, []).map(row => row.name)).toEqual(['a', 'b', 'c', 'd']);
    expect(pendingReleaseTargets(rows, results).map(row => row.name)).toEqual(['b', 'd']);
    expect(pendingReleaseTargets(rows.slice(0, 1), results)).toEqual([]);
  });

  test('ineligible rows are refused without making an API request', async () => {
    const calls = [];
    const api = { createGithubRelease: async (...args) => { calls.push(args); } };
    const rows = [pushed('a', { created: { annotated: false } }), pushed('b', { status: 'push-failed', pushed: null })];
    const results = await createGithubReleases(rows, { tag: 'v1', notes: '' }, api);
    expect(results.map(row => row.status)).toEqual(['refused', 'refused']);
    expect(calls).toEqual([]);
  });

  test('explicit publication and structured rate-limit messages reach the result', async () => {
    const calls = [];
    const api = { createGithubRelease: async (...args) => { calls.push(args); throw { kind: 'rateLimited', resetAt: '2026-10-07T00:00:00Z', message: 'GitHub rate limit reached; retry after 2026-10-07T00:00:00Z' }; } };
    const [result] = await createGithubReleases([pushed('a')], { tag: 'v1', notes: '', draft: false }, api);
    expect(calls[0][3]).toBe(false);
    expect(result.error).toContain('retry after 2026-10-07');
  });

  test('the IPC wrapper sends the tag, notes and default draft flag', async () => {
    const { api } = await import('./api.ts');
    const calls = [];
    await withIpc((command, args) => {
      calls.push([command, args]);
      return Promise.resolve({ id: 33, url: 'https://gitint.company.com/admin/a/releases/33', draft: args.draft });
    }, async () => {
      expect((await api.createGithubRelease('/r/a', 'v1', 'Tag message')).draft).toBe(true);
      expect((await api.createGithubRelease('/r/a', 'v1', '', false, 'upstream')).draft).toBe(false);
    });
    expect(calls[0]).toEqual(['create_github_release', { path: '/r/a', tag: 'v1', notes: 'Tag message', draft: true, remote: null }]);
    expect(calls[1]).toEqual(['create_github_release', { path: '/r/a', tag: 'v1', notes: '', draft: false, remote: 'upstream' }]);
  });
});
