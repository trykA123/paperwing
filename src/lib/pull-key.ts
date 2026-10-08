import type { LocalStatus, SetItem, Source } from './api';
import { isCloned } from './formation';
import type { PullKey } from './pull-support';
import { isRepoDisabled } from './source-status';

/** The branch a pull request would start from: a cloned repository on a branch, not a tag or a detached commit. A disabled source has none. */
export function choosePullKey(item: Pick<SetItem, 'repoId'>, path: string, local: LocalStatus | undefined, sources: readonly Pick<Source, 'id' | 'enabled'>[]): PullKey | null {
  if (isRepoDisabled(item.repoId, sources)) return null;
  return isCloned(local) && local?.branch ? { path, branch: local.branch } : null;
}
