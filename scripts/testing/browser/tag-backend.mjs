import { basename } from 'node:path';
import { git } from './fixture.mjs';

export const calls = [];
const tryGit = (dir, ...args) => { try { return git(dir, ...args); } catch (error) { return { failed: String(error.stderr ?? error.message) }; } };
const failure = out => out.failed.trim().split('\n').find(Boolean) ?? 'git failed';
const lines = text => text.split('\n').filter(Boolean);
const objectOf = (path, name) => { const out = tryGit(path, 'rev-parse', '--verify', '--quiet', `refs/tags/${name}`); return typeof out === 'string' ? out.trim() : null; };

function validName(path, name) {
  const ok = name && !/^head$/i.test(name) && typeof tryGit(path, 'check-ref-format', `refs/tags/${name}`) === 'string';
  if (!ok) throw new Error('Invalid tag name');
}

function validRemote(path, remote) {
  if (!lines(git(path, 'remote')).includes(remote)) throw new Error(`There is no remote named ${remote}`);
}

export function list(path) {
  calls.push({ command: 'list_tags' });
  return lines(git(path, 'for-each-ref', '--sort=refname', '--format=%(refname:strip=2)%1f%(objecttype)%1f%(objectname)%1f%(*objectname)%1f%(contents:subject)', 'refs/tags')).map(line => {
    const [name, kind, object, peeled, subject] = line.split('\x1f');
    const annotated = kind === 'tag';
    return { name, commit: annotated ? peeled : object, object, annotated, subject: annotated ? subject : null };
  });
}

export function create(path, request) {
  calls.push({ command: 'create_tag', request });
  validName(path, request.name);
  const message = request.message?.trim() || null;
  if (!message && tryGit(path, 'config', '--type=bool', '--get', 'tag.gpgSign').trim?.() === 'true') throw new Error('This repository signs tags; enter a tag message');
  const commit = git(path, 'rev-parse', '--verify', `${request.target || 'HEAD'}^{commit}`).trim();
  const previous = objectOf(path, request.name);
  if (previous && !request.moveExisting) throw new Error(`A tag named ${request.name} already exists`);
  git(path, 'tag', ...(previous ? ['--force'] : []), ...(message ? ['-a', '--cleanup=whitespace', '-m', message] : []), request.name, commit);
  return { name: request.name, commit, object: objectOf(path, request.name), annotated: !!message, previousObject: previous };
}

export function push(path, remote, name, lease) {
  calls.push({ command: 'push_tag', remote, lease });
  validName(path, name);
  validRemote(path, remote);
  if (!objectOf(path, name)) throw new Error(`There is no local tag named ${name}`);
  const ref = `refs/tags/${name}`;
  const out = tryGit(path, 'push', '-q', ...(lease === null ? [] : [`--force-with-lease=${ref}:${lease}`]), remote, ref);
  if (typeof out !== 'string') throw new Error(failure(out));
  return { remote, name, forced: lease !== null };
}

export function remove(path, name) {
  calls.push({ command: 'delete_tag' });
  validName(path, name);
  const object = objectOf(path, name);
  if (!object) throw new Error(`There is no local tag named ${name}`);
  git(path, 'tag', '--delete', name);
  return { name, object };
}

export function history(path) {
  const log = lines(git(path, 'log', '-8', '--format=%H%x1f%h%x1f%s%x1f%an%x1f%aI')).map(line => {
    const [sha, short, subject, author, date] = line.split('\x1f');
    return { sha, short, subject, author, date };
  });
  return { kind: 'noUpstream', branch: git(path, 'branch', '--show-current').trim(), upstream: null, uncommitted: 0, local: log, localTotal: log.length, origin: [], originTotal: 0, base: null, below: [] };
}

export function createRelease(path, tag, notes, draft = true, remote = 'origin') {
  calls.push({ command: 'create_github_release', path, tag, notes, draft, remote });
  validName(path, tag);
  validRemote(path, remote);
  const object = objectOf(path, tag);
  if (!object || git(path, 'cat-file', '-t', object).trim() !== 'tag') throw new Error('GitHub releases require an annotated tag');
  const published = git(path, 'ls-remote', '--refs', remote, `refs/tags/${tag}`).trim();
  if (published !== `${object}\trefs/tags/${tag}`) throw new Error(`Tag ${tag} is not on the remote at the expected object`);
  if (basename(path) === 'gamma') throw new Error('GitHub access denied; check token repository permissions');
  return { id: 33, url: `https://gitint.company.com/admin/${basename(path)}/releases/33`, draft };
}
