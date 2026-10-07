import { git } from './fixture.mjs';

const CAP = 20000;
export const calls = [];
const tryGit = (dir, ...args) => { try { return git(dir, ...args); } catch (error) { return { failed: String(error.stderr ?? error.message) }; } };
const lines = text => text.split('\n').filter(Boolean);
const top = path => { try { return git(path, 'rev-parse', '--verify', '--quiet', 'refs/stash').trim() || null; } catch { return null; } };
const dirty = (path, untracked) => lines(git(path, 'status', '--porcelain', untracked ? '--untracked-files=all' : '--untracked-files=no')).length > 0;
const conflicted = path => lines(git(path, 'diff', '--name-only', '--diff-filter=U'));

export function list(path) {
  return lines(git(path, 'stash', 'list', '--format=%gd%x09%H%x09%ct%x09%gs')).map((line, index) => {
    const [reference, oid, time, ...rest] = line.split('\t');
    const subject = rest.join('\t');
    const match = /^(WIP on |On )([^:]+): (.*)$/.exec(subject);
    return { index, reference, oid, message: match ? (match[1] === 'On ' ? match[3] : subject) : subject, branch: match?.[2] ?? null, createdAt: Number(time) };
  });
}

export function push(path, message, untracked) {
  calls.push({ command: 'stash_push', untracked });
  if (!dirty(path, untracked)) return { stashed: null, nothingToStash: true };
  const before = top(path);
  git(path, 'stash', 'push', ...(untracked ? ['--include-untracked'] : []), ...(message ? ['-m', message] : []));
  const after = top(path);
  return after === before ? { stashed: null, nothingToStash: true } : { stashed: after, nothingToStash: false };
}

function restore(path, oid, verb) {
  calls.push({ command: `stash_${verb}`, oid });
  const entry = list(path).find(item => item.oid === oid);
  if (!entry) throw new Error('That stash no longer exists; refresh the list');
  if (conflicted(path).length) throw new Error('Resolve the current conflicts first');
  const out = tryGit(path, 'stash', verb, '--index', verb === 'pop' ? entry.reference : oid);
  const kept = list(path).some(item => item.oid === oid);
  if (typeof out === 'string') return { applied: true, stashKept: kept, indexRestored: true, conflicted: [], error: null };
  return { applied: false, stashKept: kept, indexRestored: true, conflicted: conflicted(path), error: out.failed.trim().split('\n')[0] };
}

export const apply = (path, oid) => restore(path, oid, 'apply');
export const pop = (path, oid) => restore(path, oid, 'pop');

export function drop(path, oid) {
  calls.push({ command: 'stash_drop', oid });
  const entry = list(path).find(item => item.oid === oid);
  if (!entry) throw new Error('That stash no longer exists; refresh the list');
  git(path, 'stash', 'drop', entry.reference);
}

export function show(path, oid) {
  const hasUntracked = tryGit(path, 'rev-parse', '--verify', '--quiet', `${oid}^3`).failed === undefined;
  const patch = git(path, 'stash', 'show', '-p', '--no-color', ...(hasUntracked ? ['--include-untracked'] : []), oid);
  const truncated = patch.length > CAP;
  return { patch: patch.slice(0, CAP), truncated, hasUntracked, notice: truncated ? 'Stash too large to preview' : null };
}

export function switchWithStash(path, branch) {
  calls.push({ command: 'switch_with_stash', path, branch });
  if (tryGit(path, 'rev-parse', '--verify', '--quiet', `refs/heads/${branch}`).failed !== undefined) throw new Error(`There is no branch named ${branch}`);
  if (git(path, 'branch', '--show-current').trim() === branch) return { stashed: null, switched: true, error: null };
  let stashed = null;
  if (dirty(path, true)) stashed = push(path, `Skein: before switching to ${branch}`, true).stashed;
  const out = tryGit(path, 'switch', branch);
  return typeof out === 'string' ? { stashed, switched: true, error: null } : { stashed, switched: false, error: out.failed.trim() };
}
