import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { git } from './fixture.mjs';

const commit = (dir, file, text, message) => {
  mkdirSync(join(dir, file, '..'), { recursive: true });
  writeFileSync(join(dir, file), text);
  git(dir, 'add', '-A');
  git(dir, 'commit', '-q', '-m', message);
};

function base(dir, conflictOnRelease) {
  mkdirSync(dir, { recursive: true });
  git(dir, 'init', '-q', '-b', 'main');
  commit(dir, 'src/app.ts', 'export const app = 1;\nexport const two = 2;\n', 'Add app');
  commit(dir, 'conflict.txt', 'line one\nline two\n', 'Add conflict file');
  git(dir, 'checkout', '-q', '-b', 'release');
  commit(dir, 'release.txt', 'release notes\n', 'Release notes');
  if (conflictOnRelease) commit(dir, 'conflict.txt', 'line one changed on release\nline two\n', 'Change on release');
  git(dir, 'checkout', '-q', 'main');
}

/** Three repositories on main with a release branch: alpha and beta dirty, gamma clean. beta conflicts on restore. */
export function makeStashSet(root) {
  const paths = { alpha: join(root, 'alpha'), beta: join(root, 'beta'), gamma: join(root, 'gamma') };
  base(paths.alpha, false); base(paths.beta, true); base(paths.gamma, false);
  writeFileSync(join(paths.alpha, 'src/app.ts'), 'export const app = 1;\nexport const two = 22;\n');
  writeFileSync(join(paths.alpha, 'notes-untracked.txt'), 'scratch\n');
  writeFileSync(join(paths.beta, 'conflict.txt'), 'line one changed on main\nline two\n');
  writeFileSync(join(paths.gamma, 'big.txt'), Array.from({ length: 3000 }, (_, index) => `generated line ${index}`).join('\n') + '\n');
  git(paths.gamma, 'add', 'big.txt');
  git(paths.gamma, 'stash', 'push', '-q', '-m', 'Large generated file');
  writeFileSync(join(paths.gamma, 'src/app.ts'), 'export const app = 3;\nexport const two = 2;\n');
  git(paths.gamma, 'stash', 'push', '-q', '-m', 'Tweak app value');
  return paths;
}
