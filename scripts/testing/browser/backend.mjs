import { git } from './fixture.mjs';

const PROTECTED = ['main', 'master'];
const lines = text => text.split('\n').filter(Boolean);
const tryGit = (dir, ...args) => { try { return git(dir, ...args); } catch { return null; } };

export function mergedBranches(path) {
  const current = git(path, 'branch', '--show-current').trim() || null;
  const remote = tryGit(path, 'remote')?.trim().split('\n')[0] || null;
  const merged = new Set(lines(git(path, 'for-each-ref', '--merged=main', '--format=%(refname)', 'refs/heads')));
  const guarded = new Set([...PROTECTED, 'main', current]);
  const rows = lines(git(path, 'for-each-ref', '--format=%(refname:short)%00%(objectname)%00%(upstream:short)%00%(upstream:track)%00%(worktreepath)%00%(committerdate:unix)%00%(contents:subject)', 'refs/heads'));
  const local = rows.map(row => row.split('\0')).filter(row => !guarded.has(row[0])).map(row => ({
    name: row[0], oid: row[1], merged: merged.has(`refs/heads/${row[0]}`), upstream: row[2] || null, upstreamGone: !!row[2] && row[3].includes('gone'),
    inWorktree: !!row[4], lastCommit: Number(row[5]), subject: row[6],
  }));
  let remoteBranches = [];
  if (remote) {
    const prefix = `refs/remotes/${remote}/`;
    remoteBranches = lines(git(path, 'for-each-ref', `--merged=${remote}/main`, '--format=%(refname)%00%(objectname)%00%(committerdate:unix)%00%(contents:subject)', prefix))
      .map(row => row.split('\0')).map(row => ({ name: row[0].slice(prefix.length), oid: row[1], lastCommit: Number(row[2]), subject: row[3] }))
      .filter(row => row.name !== 'HEAD' && !guarded.has(row.name));
  }
  return { base: 'main', baseName: 'main', remote, remoteBase: remote ? `${remote}/main` : null, current, local, remoteBranches };
}

export function deleteMerged(path, names, expected) {
  return names.map((name, index) => {
    const tip = tryGit(path, 'rev-parse', `refs/heads/${name}`)?.trim();
    if (tip !== expected[index]) return { name, deleted: false, error: `${name} moved since the preview` };
    try { git(path, 'branch', '-d', name); return { name, deleted: true, error: null }; }
    catch (error) { return { name, deleted: false, error: String(error.stderr ?? error).trim() }; }
  });
}

export function deleteRemote(path, remote, names, expected) {
  const outcomes = names.map((name, index) => {
    const tip = tryGit(path, 'rev-parse', `refs/remotes/${remote}/${name}`)?.trim();
    return tip === expected[index] && !PROTECTED.includes(name) ? { name, deleted: false, error: null } : { name, deleted: false, error: `${name} changed or is protected` };
  });
  const ok = outcomes.filter(outcome => !outcome.error);
  if (ok.length) {
    git(path, 'push', '-q', remote, '--delete', ...ok.map(outcome => outcome.name));
    ok.forEach(outcome => { outcome.deleted = true; });
  }
  return outcomes;
}

export function localStatus(paths) {
  return paths.map(path => ({
    path, exists: true, repo: true, branch: git(path, 'branch', '--show-current').trim() || null, tag: null, sha: git(path, 'rev-parse', 'HEAD').trim(),
    upstream: null, ahead: 0, behind: 0, dirty: 0, error: null,
  }));
}

const GREP_FLAGS = { fixed: '-F', basic: '-G', perl: '-P' };

function parseGrep(output, hasRef, context) {
  const matches = [];
  const pending = [];
  for (const line of output.split('\n')) {
    const text = hasRef ? line.replace(/^[^:]+:/, '') : line;
    const hit = /^(.+?):(\d+):(\d+):(.*)$/.exec(text);
    const near = /^(.+?)-(\d+)-(.*)$/.exec(text);
    if (hit) matches.push({ path: hit[1], line: Number(hit[2]), column: Number(hit[3]), text: hit[4], context: [] });
    else if (context && near) pending.push({ path: near[1], line: Number(near[2]), text: near[3] });
  }
  for (const match of matches) match.context = pending.filter(entry => entry.path === match.path && Math.abs(entry.line - match.line) <= context).map(({ line, text }) => ({ line, text }));
  return matches;
}

export function grepRepo(request, target) {
  const args = ['grep', '-n', '--column', '-I', GREP_FLAGS[request.mode ?? 'fixed']];
  if (request.ignoreCase) args.push('-i');
  if (request.wholeWord) args.push('-w');
  if (request.context) args.push(`-C${request.context}`);
  if (request.untracked && !target.gitRef) args.push('--untracked');
  args.push('-e', request.pattern);
  if (target.gitRef) args.push(target.gitRef);
  if (request.pathspecs?.length) args.push('--', ...request.pathspecs);
  const raw = tryGit(target.path, ...args);
  if (raw === null) return [];
  return parseGrep(raw, !!target.gitRef, request.context);
}
