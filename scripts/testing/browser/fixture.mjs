import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

export const git = (dir, ...args) => execFileSync('git', ['-C', dir, ...args], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
const env = { GIT_AUTHOR_NAME: 'admin', GIT_AUTHOR_EMAIL: 'admin@example.test', GIT_COMMITTER_NAME: 'admin', GIT_COMMITTER_EMAIL: 'admin@example.test' };
Object.assign(process.env, env);

function commit(dir, file, text, message) {
  mkdirSync(join(dir, file, '..'), { recursive: true });
  writeFileSync(join(dir, file), text);
  git(dir, 'add', '-A');
  git(dir, 'commit', '-q', '-m', message);
}

/** A repository with merged, unmerged, upstream-gone, worktree and current branches; `remote` adds a bare origin. */
export function makeRepo(dir, { remote = null, todo = 3 } = {}) {
  mkdirSync(dir, { recursive: true });
  git(dir, 'init', '-q', '-b', 'main');
  const lines = Array.from({ length: todo }, (_, index) => `// TODO item ${index + 1}\nexport const value${index} = ${index};`).join('\n');
  commit(dir, 'src/app.ts', `${lines}\n`, 'Add app');
  commit(dir, 'README.md', '# Fixture\nTODO document this\n', 'Add readme');
  for (const name of ['feature/done', 'feature/two', 'dev', 'wt-branch', 'gone']) git(dir, 'branch', name);
  git(dir, 'checkout', '-q', '-b', 'wip');
  commit(dir, 'wip.txt', 'work in progress\n', 'Unfinished work');
  git(dir, 'checkout', '-q', 'main');
  commit(dir, 'src/more.ts', 'export const more = 1; // TODO later\n', 'More code');
  git(dir, 'worktree', 'add', '-q', `${dir}-wt`, 'wt-branch');
  if (remote) {
    mkdirSync(remote, { recursive: true });
    execFileSync('git', ['init', '-q', '--bare', '-b', 'main', remote]);
    git(dir, 'remote', 'add', 'origin', remote);
    git(dir, 'branch', 'old-remote');
    for (const name of ['main', 'feature/done', 'old-remote', 'wip', 'gone']) git(dir, 'push', '-q', 'origin', name);
    git(dir, 'branch', '-u', 'origin/gone', 'gone');
    execFileSync('git', ['--git-dir', remote, 'branch', '-q', '-D', 'gone']);
    git(dir, 'fetch', '-q', '--prune');
    git(dir, 'remote', 'set-head', 'origin', 'main');
    git(dir, 'branch', '-D', 'old-remote');
  }
  git(dir, 'checkout', '-q', 'dev');
}

export function moveTip(dir, branch) {
  const tree = git(dir, 'rev-parse', `${branch}^{tree}`).trim();
  const parent = git(dir, 'rev-parse', branch).trim();
  const next = git(dir, 'commit-tree', tree, '-p', parent, '-m', 'Moved after the preview').trim();
  git(dir, 'update-ref', `refs/heads/${branch}`, next);
}

/** Local default branch is master; the remote has main (ahead) and master, and no origin/HEAD. */
export function makeDivergentRepo(dir, remote) {
  mkdirSync(dir, { recursive: true });
  mkdirSync(remote, { recursive: true });
  execFileSync('git', ['init', '-q', '--bare', '-b', 'main', remote]);
  git(dir, 'init', '-q', '-b', 'master');
  commit(dir, 'src/app.ts', '// TODO base\n', 'Base');
  git(dir, 'branch', 'topic');
  git(dir, 'remote', 'add', 'origin', remote);
  git(dir, 'push', '-q', 'origin', 'master');
  git(dir, 'checkout', '-q', '-b', 'main');
  commit(dir, 'src/next.ts', '// TODO next\n', 'Main moves ahead');
  git(dir, 'branch', 'rel-old');
  for (const name of ['main', 'rel-old']) git(dir, 'push', '-q', 'origin', name);
  git(dir, 'checkout', '-q', 'master');
  git(dir, 'branch', '-D', 'main', 'rel-old');
  git(dir, 'checkout', '-q', '-b', 'work');
  git(dir, 'fetch', '-q', 'origin');
}

export function makeCapRepo(dir, matches = 300) {
  mkdirSync(dir, { recursive: true });
  git(dir, 'init', '-q', '-b', 'main');
  commit(dir, 'src/big.ts', Array.from({ length: matches }, (_, index) => `const v${index} = ${index}; // TODO ${index}`).join('\n') + '\n', 'Many TODOs');
}
