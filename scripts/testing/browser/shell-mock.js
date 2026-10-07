(() => {
  const cfg = window.__AUDIT || {};
  const N = cfg.repos ?? 800;
  const platform = cfg.platform || 'windows';
  const sep = platform === 'windows' ? '\\' : '/';
  const orgs = ['platform', 'payments', 'mobile', 'data-eng'];
  const words = ['api','gateway','billing','auth','ledger','ios','android','web','docs','infra','etl','search','notify','admin','cli','sdk','proxy','queue','report','sync'];
  const hash = s => { let h = 2166136261; for (const c of s) { h ^= c.charCodeAt(0); h = Math.imul(h, 16777619); } return h >>> 0; };
  const repos = Array.from({ length: N }, (_, i) => {
    const org = orgs[i % orgs.length], name = `${words[i % words.length]}-${words[(i * 7 + 3) % words.length]}${i >= 20 ? '-' + i : ''}`;
    return { id: `s1:${org}/${name}`, source: 's1', org, name, description: i % 5 === 0 ? '' : `Service ${name} for the ${org} team`, url: `https://git.acme.example/${org}/${name}.git`, defaultBranch: 'main', pushedAt: new Date(Date.now() - i * 3600e3 * 7).toISOString(), archived: i % 53 === 0 };
  });
  const ossRepos = ['cli', 'sdk', 'docs', 'web', 'api', 'ui'].map(name => ({ id: `s2:acme-oss/${name}`, source: 's2', org: 'acme-oss', name, description: `Open source ${name}`, url: `https://github.com/acme-oss/${name}.git`, defaultBranch: 'main', pushedAt: new Date().toISOString(), archived: false }));
  const setItems = (count, offset = 0, from = repos) => from.slice(offset, offset + count).map((r, i) => ({ id: `it${from === repos ? '' : 'o'}${offset + i}`, repoId: r.id, url: r.url, org: r.org, name: r.name, ref: { type: 'branch', name: i % 11 === 0 ? 'release/2.4' : 'main' }, on: i % 9 !== 0 }));
  const empty = cfg.emptySets;
  const workspace = {
    sets: empty ? [{ id: 'a', name: 'My first set', items: [] }] : [
      { id: 'a', name: cfg.bigSet ? 'All services (800)' : 'Release train', items: [...setItems(cfg.bigSet ? N : 24), ...(cfg.mixed ? setItems(6, 0, ossRepos).map(item => ({ ...item, id: `m${item.id}` })) : [])] },
      { id: 'b', name: 'Mobile hotfix', items: setItems(6, 0, ossRepos) },
      { id: 'c', name: 'Empty set', items: [] },
    ],
    stars: [repos[1].id, repos[5].id], activeSet: 'a', root: platform === 'windows' ? 'C:\\Dev\\repos' : '/home/dev/repos', layout: 'flat',
    pathTemplate: '{org}\\{folder}', cols: { repo: 210, checkout: 190, local: 220, status: 170 }, shallow: false, parallel: 4, onExisting: 'fetch', pageSize: 25, rightWidth: 380,
    theme: cfg.theme || 'system', uiFont: 'geist', codeFont: 'geist-mono',
    shell: { version: 1, sidebarWidth: 250, sidebarVisible: true, rightVisible: true, section: cfg.section || 'sets' },
  };
  const cap = { supported: true, reason: null };
  const caps = { readCompare: cap, edit: cap, copy: cap, recovery: cap, trash: cap };
  const sources = cfg.sources ?? (cfg.noSources ? [] : [{ id: 's1', name: 'Acme GHE', kind: 'ghe', host: 'git.acme.example', orgs, urls: [] }, { id: 's2', name: 'github.com', kind: 'github', host: 'github.com', orgs: ['acme-oss'], urls: [] }]);
  const callbacks = {}; let cbId = 1; const listeners = {};
  window.__emit = (event, payload) => (listeners[event] || []).forEach(id => callbacks[id]?.({ event, id: 0, payload }));
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const wait = cfg.latency ?? 0;
  const commit = (i, s) => ({ sha: (hash(s + i).toString(16) + '0000000000000000').slice(0, 40), short: hash(s + i).toString(16).slice(0, 7), subject: ['Fix retry backoff in queue worker','Bump deps','Add ledger export','Refactor auth middleware','Handle 429 from gateway','Docs: update runbook'][i % 6], author: ['Ana','Mihai','Sam','Ioana'][i % 4], date: new Date(Date.now() - i * 86400e3).toISOString() });
  const files = ['src/main.rs','src/lib/auth.ts','README.md','Cargo.toml','docs/runbook.md','src/queue/worker.ts','src/queue/retry.ts','package.json','assets/logo.png','tests/auth.test.ts'].map((p, i) => ({ id: 'f' + i, path: p, left: i === 3 ? null : { kind: 'file', size: 1200 + i * 90, modifiedMs: Date.now() - i * 1e6, reason: null, source: 'commitBlob' }, right: i === 2 ? null : { kind: 'file', size: 1300 + i * 80, modifiedMs: Date.now(), reason: null, source: 'workingTree' },
    rawStatus: i === 2 ? 'leftOnly' : i === 3 ? 'rightOnly' : i % 4 === 0 ? 'same' : 'different', displayStatus: i === 2 ? 'leftOnly' : i === 3 ? 'rightOnly' : i % 4 === 0 ? 'same' : 'different', rawLines: { added: i * 3, removed: i }, displayLines: { added: i * 3, removed: i }, binary: p.endsWith('png'), rename: null, reason: null }));
  const summary = { same: 3, different: 5, leftOnly: 1, rightOnly: 1, unavailable: 0, typeConflict: 0, total: 10 };
  const handlers = {
    platform_info: () => ({ platform, separator: sep, capabilities: caps, credentials: { backend: 'windowsCredentialManager', persistent: true, supported: true, reason: null } }),
    probe_root: ({ root }) => ({ root, valid: true, reason: null, identity: 'id1', casePolicy: 'insensitive', capabilities: caps }),
    path_identities: ({ paths }) => paths.map(path => ({ path, identity: null, exists: true, reason: null })),
    load_settings: () => ({ sources, workspace }),
    save_settings: () => null,
    source_revision: () => 1, has_token: () => true, credential_status: ({ sourceId }) => ({ sourceId, backend: 'windowsCredentialManager', state: cfg.credMissing ? 'missing' : 'saved', revision: 1, reason: null }),
    list_cached_repos: () => null,
    list_repos: async ({ source }) => { await sleep(cfg.listDelay ?? 300); if (cfg.listError) throw 'HTTP 401 Unauthorized: token rejected by git.acme.example'; return source.id === 's1' ? { repos, fetchedAt: Date.now(), errors: [], warnings: [] } : { repos: ossRepos, fetchedAt: Date.now(), errors: [] }; },
    list_user_orgs: () => orgs, test_source: () => 'Connected as admin',
    paths_exist: ({ paths }) => paths.map(() => true),
    local_status: ({ paths }) => paths.map(path => { const h = hash(path), k = h % 10; return { path, exists: k !== 0, repo: k !== 0, branch: k === 1 ? null : 'main', tag: null, sha: h.toString(16).padStart(40, '0').slice(0, 40), upstream: 'origin/main', ahead: k === 2 || k === 3 ? (h >> 4) % 4 + 1 : 0, behind: k === 3 || k === 4 || k === 5 ? (h >> 6) % 9 + 1 : 0, dirty: k === 6 || k === 3 ? (h >> 3) % 12 + 1 : 0, error: k === 7 && cfg.rowErrors ? 'fatal: unable to access remote' : null }; }),
    get_refs_many: ({ urls }) => urls.map(url => ({ url, branches: ['main', 'develop', 'release/2.4'], tags: ['v2.3.0', 'v2.4.0'], branchShas: [commit(0,'x').sha, commit(1,'x').sha, commit(2,'x').sha], tagShas: [commit(3,'x').sha, commit(4,'x').sha], branchLabels: ['main','develop','release/2.4'], tagLabels: ['v2.3.0','v2.4.0'], error: null })),
    activity_snapshot: () => cfg.activity ? [
      { id: 'x1', context: 'platform/api-gateway', argv: ['git','fetch','--prune','origin'], sequence: 1, startedAt: Date.now() - 9000, elapsedMs: 1840, state: 'completed', exitCode: 0, output: [], truncated: false, stdoutBytes: 0, stderrBytes: 120 },
      { id: 'x2', context: 'payments/ledger-sdk', argv: ['git','pull','--ff-only'], sequence: 2, startedAt: Date.now() - 8000, elapsedMs: 940, state: 'failed', exitCode: 128, output: [{ sequence: 1, stream: 'stderr', text: 'fatal: Not possible to fast-forward, aborting.' }], truncated: false, stdoutBytes: 0, stderrBytes: 48 },
    ] : [],
    clear_activity: () => ({ running: [], retained: [], through: 0 }),
    repository_tree: () => ({ branches: [{ name: 'main', sha: 'a'.repeat(40), current: true, symbolic: '' }, { name: 'develop', sha: 'b'.repeat(40), current: false, symbolic: '' }], tags: [{ name: 'v2.4.0', sha: 'd'.repeat(40), current: false, symbolic: '' }], remotes: [{ name: 'origin', urls: ['https://git.acme.example/platform/api.git'], refs: [{ name: 'origin/main', sha: 'a'.repeat(40), current: false, symbolic: '' }] }], stashes: [{ name: 'stash@{0}', sha: 'f'.repeat(40), subject: 'WIP on main' }], submodules: [] }),
    repository_history: ({ path }) => ({ kind: 'tracking', branch: 'main', upstream: 'origin/main', uncommitted: 4, local: [0,1].map(i => commit(i, path)), localTotal: 2, origin: [2,3,4].map(i => commit(i, path)), originTotal: 3, base: commit(5, path), below: [6,7,8].map(i => commit(i, path)) }),
    repo_changes: () => ({ branch: 'main', detached: false, unborn: false, head: 'a'.repeat(40), files: [{ path: 'src/main.rs', origPath: null, index: 'M', worktree: '.', kind: 'ordinary', stagedAdded: 4, stagedRemoved: 1 }, { path: 'src/new.ts', origPath: null, index: '?', worktree: '?', kind: 'untracked', stagedAdded: null, stagedRemoved: null }, { path: 'README.md', origPath: null, index: '.', worktree: 'M', kind: 'ordinary', stagedAdded: null, stagedRemoved: null }], truncated: false, skipped: 0, author: 'admin', authorError: null, stagedFiles: 1, stagedAdded: 4, stagedRemoved: 1 }),
    list_tags: () => [], pull_for_branch: ({ path }) => {
      const h = hash(path), k = h % 10;
      if (k > 6) return null;
      const checks = k === 1 ? { state: 'failure', success: 3, failure: 2, pending: 0, total: 5 } : { state: 'success', success: 5, failure: 0, pending: 0, total: 5 };
      return { number: 4100 + (h % 90), title: `Retry 429 responses in ${path.split(/[\\/]/).pop()}`, url: `https://git.acme.example/pull/${h % 90}`, state: k === 2 ? 'draft' : 'open', base: 'main', headSha: 'a'.repeat(40), reviewState: k === 0 || k === 3 ? 'reviewRequired' : k === 1 ? 'changesRequested' : 'approved', checks, targetRepo: path.split(/[\\/]/).slice(-2).join('/'), hasUnpushedCommits: false };
    }, get_commits: () => Array.from({length:6},(_,i)=>{const c=commit(i,'x');return {sha:c.sha,message:c.subject,author:c.author,date:c.date,parents:i<5?[commit(i+1,'x').sha]:[]};}),
    merged_branches: () => ({ base: 'main', baseName: 'main', remote: 'origin', remoteBase: 'origin/main', current: 'main', local: [{ name: 'feature/old-retry', oid: 'a'.repeat(40), merged: true, upstream: 'origin/feature/old-retry', upstreamGone: true, inWorktree: false, lastCommit: Date.now()/1000 - 9e6, subject: 'Retry tweaks' }], remoteBranches: [] }),
    comparison_open: ({ left, right }) => { window.__ep = { left, right }; return { id: 'cmp1', generation: 1 }; },
    comparison_refresh: () => ({ status: 'ready', snapshot: { id: 'cmp1', generation: 1, left: { endpoint: window.__ep.left, commit: 'a'.repeat(40), historyBasis: 'commit' }, right: { endpoint: window.__ep.right, commit: 'b'.repeat(40), historyBasis: 'workingTreeHead' }, raw: summary, display: summary, history: { available: true, reason: null, leftCount: 3, rightCount: 2, leftBasis: 'commit', rightBasis: 'workingTreeHead' }, fileCount: 10, options: { normalizeEol: true, ignoreWhitespace: false } } }),
    comparison_files: ({ offset }) => offset ? [] : files,
    comparison_commits: () => [0,1,2].map(i => ({ side: i % 2 ? 'left' : 'right', sha: commit(i,'c').sha, subject: commit(i,'c').subject, author: commit(i,'c').author, date: commit(i,'c').date })),
    comparison_content: ({ side }) => ({ generation: 1, side, kind: 'file', bytes: Array.from(new TextEncoder().encode(side === 'left' ? 'fn main() {\n  println!("a");\n}\n' : 'fn main() {\n  println!("b");\n  run();\n}\n')), binary: false }),
    comparison_close: () => true, comparison_cancel: () => true,
    recovery_list: () => cfg.recovery ? [{ id: 'r1', root: 'C:\\Dev\\repos', path: 'platform\\api\\src\\main.rs', existed: true, stage: 'applied', createdAt: Date.now() - 600000 }] : [],
    launch_request: () => [], search_capabilities: () => ({ perl: true }), search_start: () => 1, search_cancel: () => true, search_cancel_all: () => 0,
    discover_cancel_all: () => 0, start_clone: () => null,
    'plugin:event|listen': ({ event, handler }) => { (listeners[event] ||= []).push(handler); return handler; },
    'plugin:event|unlisten': () => null,
    'plugin:window|is_maximized': () => false,
    'plugin:window|is_focused': () => true,
  };
  window.isTauri = false;
  window.__TAURI_INTERNALS__ = {
    transformCallback: (cb) => { const id = cbId++; callbacks[id] = cb; return id; },
    unregisterCallback: id => delete callbacks[id],
    convertFileSrc: p => p,
    metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main', windowLabel: 'main' } },
    invoke: async (cmd, args = {}) => {
      if (wait) await sleep(wait);
      const h = handlers[cmd];
      if (!h) { (window.__missing ||= []).push(cmd); if (cmd.startsWith('plugin:window')) return null; throw `mock: unhandled ${cmd}`; }
      return await h(args);
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
})();
