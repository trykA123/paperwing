const CHECKS = {
  none: { state: 'none', success: 0, failure: 0, pending: 0, total: 0 },
  pass: { state: 'success', success: 5, failure: 0, pending: 0, total: 5 },
  fail: { state: 'failure', success: 3, failure: 2, pending: 0, total: 5 },
  wait: { state: 'pending', success: 2, failure: 0, pending: 3, total: 5 },
};
const FLAVOURS = [
  { state: 'open', reviewState: 'approved', checks: CHECKS.pass },
  { state: 'draft', reviewState: 'none', checks: CHECKS.none },
  { state: 'merged', reviewState: 'approved', checks: CHECKS.pass },
  { state: 'closed', reviewState: 'none', checks: CHECKS.none },
  { state: 'open', reviewState: 'changesRequested', checks: CHECKS.fail },
  { state: 'open', reviewState: 'reviewRequired', checks: CHECKS.wait },
  null, 'missing-source', { state: 'open', reviewState: 'approved', checks: CHECKS.none }, null, null, null,
];

export const ghes = 'ghe.example.test';
export const indexOf = path => Number(/repo-(\d+)$/.exec(path)?.[1] ?? -1);
export const hostOf = index => (index % 12 === 7 ? ghes : 'example.test');

/** Repository i sits on feature/i; every ninth is unpublished and every seventh is two commits ahead. */
export function statusFor(path) {
  const index = indexOf(path);
  return {
    path, exists: true, repo: true, branch: `feature/${index}`, tag: null, sha: index.toString(16).padStart(40, '0'),
    upstream: index % 9 === 0 ? null : `origin/feature/${index}`, ahead: index % 7 === 0 ? 2 : 0, behind: 0, dirty: 0, error: null,
  };
}

export function makePulls() {
  const state = { created: {}, calls: [], opened: [], pushed: [], limitAfter: Infinity, resetAt: new Date(Date.now() + 30 * 60_000).toISOString() };
  state.pullForBranch = path => {
    state.calls.push(path);
    if (state.calls.length > state.limitAfter) throw { kind: 'rateLimited', resetAt: state.resetAt, message: 'GitHub rate limit reached' };
    const index = indexOf(path);
    if (state.created[path]) return state.created[path];
    const flavour = FLAVOURS[index % FLAVOURS.length];
    if (flavour === 'missing-source') throw { kind: 'message', message: `Add a source for ${ghes}` };
    if (!flavour) return null;
    return {
      number: 100 + index, title: `Feature ${index}: ship the login flow`, url: `https://${hostOf(index)}/org/repo-${index}/pull/${100 + index}`, base: 'main',
      headSha: 'a'.repeat(40), targetRepo: `org/repo-${index}`, hasUnpushedCommits: false, ...flavour,
    };
  };
  state.openPullRequest = (path, request) => {
    const index = indexOf(path);
    state.opened.push({ path, request });
    if (index === 10) throw { kind: 'message', message: 'Validation Failed: A pull request already exists' };
    const made = { number: 500 + index, url: `https://${hostOf(index)}/org/repo-${index}/pull/${500 + index}`, targetRepo: index === 3 ? 'upstream/repo-3' : `org/repo-${index}`, hasUnpushedCommits: false };
    state.created[path] = { number: made.number, title: request.title, url: made.url, state: request.draft ? 'draft' : 'open', base: request.base, headSha: 'c'.repeat(40), reviewState: 'none', checks: CHECKS.none, targetRepo: made.targetRepo, hasUnpushedCommits: false };
    return made;
  };
  return state;
}

export const remoteTree = () => ({
  branches: [], tags: [], stashes: [], submodules: [],
  remotes: [{ name: 'origin', urls: [], refs: ['HEAD', 'main', 'develop', 'release'].map(name => ({ name: `origin/${name}`, sha: 'a'.repeat(40), current: false, symbolic: name === 'HEAD' ? 'refs/remotes/origin/main' : '' })) }],
});

export const lastCommit = path => ({
  kind: 'tracking', branch: `feature/${indexOf(path)}`, upstream: null, uncommitted: 0, localTotal: 1, originTotal: 0, base: null, below: [], origin: [],
  local: [{ sha: 'b'.repeat(40), short: 'bbbbbbb', subject: `Wire up login for repo ${indexOf(path)}`, author: 'admin', date: '2026-10-07T08:00:00Z' }],
});
