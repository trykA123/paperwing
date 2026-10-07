import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { git } from './fixture.mjs';

function repo(dir, remote) {
  mkdirSync(dir, { recursive: true });
  git(dir, 'init', '-q', '-b', 'main');
  for (const [file, text, message] of [['a.txt', 'one\n', 'Add a'], ['b.txt', 'two\n', 'Add b']]) {
    writeFileSync(join(dir, file), text);
    git(dir, 'add', '-A');
    git(dir, 'commit', '-q', '-m', message);
  }
  mkdirSync(remote, { recursive: true });
  execFileSync('git', ['init', '-q', '--bare', '-b', 'main', remote]);
  git(dir, 'remote', 'add', 'origin', remote);
  git(dir, 'push', '-q', 'origin', 'main');
}

export const addCommit = (dir, file, message) => {
  writeFileSync(join(dir, file), `${message}\n`);
  git(dir, 'add', '-A');
  git(dir, 'commit', '-q', '-m', message);
};

/** Three repositories with bare remotes. beta's remote path is later broken so its push fails. gamma starts with a release tag. */
export function makeTagSet(root) {
  const paths = { alpha: join(root, 'alpha'), beta: join(root, 'beta'), gamma: join(root, 'gamma') };
  const remotes = Object.fromEntries(Object.keys(paths).map(name => [name, join(root, `${name}.git`)]));
  for (const name of Object.keys(paths)) repo(paths[name], remotes[name]);
  git(paths.gamma, 'tag', '-a', '-m', 'First release', 'v1.0.0');
  git(paths.gamma, 'push', '-q', 'origin', 'v1.0.0');
  git(paths.beta, 'remote', 'set-url', 'origin', join(root, 'missing.git'));
  return { paths, remotes };
}
