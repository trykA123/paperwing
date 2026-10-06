import { invoke } from '@tauri-apps/api/core';

export type RefKind = 'branch' | 'tag' | 'commit';
export type Ref = { type: RefKind; name: string };
export type CompareRef =
  | { kind: 'branch' | 'remoteBranch' | 'tag'; name: string }
  | { kind: 'commit'; sha: string }
  | { kind: 'head' | 'workingTree' };
export type CompareEndpoint = { setId: string; itemId: string; reference: CompareRef };
export type CompareOptions = { normalizeEol: boolean; ignoreWhitespace: boolean };
export type CompareUnavailableReason = 'notCloned' | 'notRepository' | 'unsupportedEncoding' | 'unmergedIndex' | 'refreshRequired' | 'unbornHead' | 'gitCapability' | 'other';
export type CompareProblem = { kind: string; side: 'left' | 'right' | null; message: string; reason?: CompareUnavailableReason | null };
export type CompareStatus = 'same' | 'different' | 'leftOnly' | 'rightOnly' | 'typeConflict' | 'unavailable';
export type CompareEntryKind = 'file' | 'symlink' | 'gitlink' | 'directory';
export type CompareSide = {
  kind: CompareEntryKind; size: number | null; modifiedMs: number | null; reason: string | null;
  source: 'commitBlob' | 'workingTree' | 'indexGitlink' | 'untrackedRepository' | 'aggregate';
};
export type CompareLines = { added: number; removed: number };
export type CompareFile = {
  id: string; path: string; left: CompareSide | null; right: CompareSide | null;
  rawStatus: CompareStatus; displayStatus: CompareStatus; rawLines: CompareLines | null; displayLines: CompareLines | null;
  binary: boolean | null; rename: { from: string; to: string; score: string } | null; reason: string | null;
};
export type CompareSummary = {
  same: number; different: number; leftOnly: number; rightOnly: number; typeConflict: number; unavailable: number; total: number;
};
export type CompareHistory = {
  available: boolean; reason: string | null; leftCount: number | null; rightCount: number | null;
  leftBasis: 'commit' | 'workingTreeHead'; rightBasis: 'commit' | 'workingTreeHead';
};
export type CompareSnapshot = {
  id: string; generation: number;
  left: { endpoint: CompareEndpoint; commit: string; historyBasis: CompareHistory['leftBasis'] };
  right: { endpoint: CompareEndpoint; commit: string; historyBasis: CompareHistory['rightBasis'] };
  raw: CompareSummary; display: CompareSummary; history: CompareHistory; fileCount: number; options: CompareOptions;
};
export type CompareResult =
  | { status: 'ready'; snapshot: CompareSnapshot }
  | { status: 'unavailable' | 'invalidRef' | 'missingLeft' | 'missingRight' | 'networkError'; problem: CompareProblem };
export type Capability = { supported: boolean; reason: string | null };
export type Capabilities = { readCompare: Capability; edit: Capability; copy: Capability; recovery: Capability; trash: Capability };
export type CredentialBackend = 'windowsCredentialManager' | 'secretService' | 'unsupported';
export type CredentialCapability = { backend: CredentialBackend; persistent: boolean; supported: boolean; reason: string | null };
export type CredentialStatus = { sourceId: string; backend: CredentialBackend; state: 'saved' | 'missing' | 'locked' | 'unavailable' | 'permissionDenied' | 'uncertain' | 'error'; revision: number; reason: string | null };
export type PlatformInfo = { platform: 'windows' | 'linux' | 'unsupported'; separator: '/' | '\\'; capabilities: Capabilities; credentials: CredentialCapability };
export type RootSupport = { root: string; valid: boolean; reason: string | null; identity: string | null;
  casePolicy: 'unknown' | 'sensitive' | 'insensitive'; capabilities: Capabilities };
export type PathIdentity = { path: string; identity: string | null; exists: boolean; reason: string | null };
export type CompareContent = { generation: number; side: 'left' | 'right'; kind: CompareEntryKind; bytes: number[]; binary: boolean };
export type CompareCommit = { side: 'left' | 'right'; sha: string; subject: string; author: string; date: string };
export type RecoveryRecord = { id: string; root: string; path: string; existed: boolean; stage: string; createdAt: number; warning?: string | null };
export type EditFile = { ticket: string; bytes: number[]; exists: boolean };
export type CopyPreview = { id: string; files: { path: string; action: 'create' | 'overwrite'; bytes: number }[]; retained: number };
export type CopyOutcome = { path: string; state: 'applied' | 'failed' | 'notAttempted'; record: RecoveryRecord | null; error: string | null };
export type SourceKind = 'github' | 'ghe' | 'manual';
export type OnExisting = 'fetch' | 'skip' | 'reclone';
export type PageSize = 10 | 25 | 50 | 'all';

export type Source = { id: string; name: string; kind: SourceKind; host: string; orgs: string[]; urls: string[]; credentialManaged?: boolean };
export type Repo = {
  id: string; source: string; org: string; name: string; description: string;
  url: string; defaultBranch: string; pushedAt: string; archived: boolean;
};
export type RepoList = { repos: Repo[]; fetchedAt: number; errors: string[]; stale?: boolean };
export type Commit = { sha: string; message: string; author: string; date: string; parents: string[] };
export type RefsResult = {
  url: string; branches: string[]; tags: string[]; branchShas: string[]; tagShas: string[]; branchLabels?: string[]; tagLabels?: string[]; error: string | null;
};
export type LocalStatus = {
  path: string; exists: boolean; repo: boolean; branch: string | null; tag: string | null; branchLabel?: string | null; tagLabel?: string | null; sha: string;
  upstream: string | null; upstreamLabel?: string | null; ahead: number; behind: number; dirty: number; error: string | null;
};
export type Theme = 'system' | 'light' | 'dark';
export type GitAction = 'clone' | 'fetch' | 'pull' | 'switch';
export type Activity = {
  id: string; context: string; argv: string[]; sequence: number; startedAt: number; elapsedMs: number;
  state: 'running' | 'completed' | 'failed' | 'cancelled' | 'timedOut'; exitCode: number | null;
  output: { sequence: number; stream: string; text: string }[]; truncated: boolean; stdoutBytes: number; stderrBytes: number;
};
export type TreeRef = { name: string; label?: string; sha: string; current: boolean; symbolic: string };
export type RepositoryTree = {
  identity?: string;
  branches: TreeRef[]; tags: TreeRef[];
  remotes: { name: string; urls: string[]; refs: TreeRef[] }[];
  stashes: { name: string; sha: string; subject: string }[];
  submodules: { path: string; sha: string; url: string | null }[];
};

export type HistoryCommit = { sha: string; short: string; subject: string; author: string; date: string };
export type HistoryKind = 'tracking' | 'noUpstream' | 'upstreamGone' | 'detached' | 'unborn';
export type RepositoryHistory = {
  kind: HistoryKind; branch: string | null; upstream: string | null; uncommitted: number;
  local: HistoryCommit[]; localTotal: number; origin: HistoryCommit[]; originTotal: number; base: HistoryCommit | null; below: HistoryCommit[];
};

export type ChangeFile = {
  path: string; origPath: string | null; index: string; worktree: string;
  kind: 'ordinary' | 'renamed' | 'unmerged' | 'untracked'; stagedAdded: number | null; stagedRemoved: number | null;
};
export type RepoChanges = {
  branch: string | null; detached: boolean; unborn: boolean; head: string; files: ChangeFile[]; truncated: boolean; skipped: number;
  author: string | null; authorError: string | null; stagedFiles: number; stagedAdded: number; stagedRemoved: number;
};
export type DiffArea = 'staged' | 'unstaged' | 'untracked';
export type TrashOutcome = { itemId: string; path: string; state: 'trashed' | 'missing' | 'skipped' | 'failed'; reason: string | null };
export type ChangeContent = { original: string; modified: string; originalLabel: string; modifiedLabel: string; binary: boolean };
export type StashEntry = { index: number; reference: string; oid: string; message: string; branch: string | null; createdAt: number };
export type StashPushOutcome = { stashed: string | null; nothingToStash: boolean };
export type StashRestoreOutcome = { applied: boolean; stashKept: boolean; conflicted: string[]; error: string | null };
export type StashDiff = { patch: string; truncated: boolean; hasUntracked: boolean };
export type SwitchStashOutcome = { stashed: string | null; switched: boolean; error: string | null };

export type SetItem = {
  id: string; repoId: string; url: string; org: string; name: string; ref: Ref; on: boolean;
  /** Clone into this folder instead of the repo name (lets one repo be cloned several times). */
  folder?: string;
};
export type RepoSet = { id: string; name: string; items: SetItem[] };
export type ColWidths = { repo: number; checkout: number; local: number; status: number };
export type ShellLayout = { version: 1; sidebarWidth: number; sidebarVisible: boolean; rightVisible: boolean };
export type Workspace = {
  sets: RepoSet[]; stars: string[]; activeSet: string; root: string; layout: 'flat' | 'custom'; pathTemplate: string;
  cols: ColWidths;
  shallow: boolean; parallel: number; onExisting: OnExisting; pageSize: PageSize; rightWidth: number;
  theme: Theme; uiFont: string; codeFont: string;
  shell: ShellLayout;
};

export type Phase = 'queued' | 'resolving' | 'cloning' | 'fetching' | 'checkout' | 'done' | 'failed' | 'skipped';
export type Progress = { id: string; phase: Phase; pct: number; msg: string };
export type CloneJob = { id: string; url: string; dest: string; refType: RefKind; refName: string };
export type CloneOpts = { parallel: number; shallow: boolean; onExisting: OnExisting };

export type LaunchAction = { kind: 'openFolder'; path: string } | { kind: 'compareFolders'; left: string; right: string };
export type LaunchRequest = { action: LaunchAction | null; ignored: { arg: string; reason: string }[] };
export type FoundRepoKind = 'normal' | 'worktree' | 'bare' | 'submodule';
export type FoundRepo = { path: string; name: string; kind: FoundRepoKind; branch: string | null; detached: boolean; parent: string | null };
export type DiscoverBatch = { id: number; repos: FoundRepo[] };
export type DiscoverSummary = {
  repositories: number; directories: number; unreadable: number; linksSkipped: number;
  capped: 'directories' | 'repositories' | null; cancelled: boolean;
};
export type DiscoverDone = { id: number; summary: DiscoverSummary };
export const events = { launchRequest: 'launch-request', discoverBatch: 'discover-batch', discoverDone: 'discover-done' } as const;

export const api = {
  platformInfo: () => invoke<PlatformInfo>('platform_info'),
  probeRoot: (root: string) => invoke<RootSupport>('probe_root', { root }),
  pathIdentities: (paths: string[]) => invoke<PathIdentity[]>('path_identities', { paths }),
  copyPreview: (id: string, generation: number, fileId: string, side: 'left' | 'right') => invoke<CopyPreview>('copy_preview', { id, generation, fileId, side }),
  copyApply: (id: string, confirmed: boolean) => invoke<CopyOutcome[]>('copy_apply', { id, confirmed }),
  copyCancel: (id: string) => invoke<boolean>('copy_cancel', { id }),
  editOpen: (id: string, generation: number, fileId: string, side: 'left' | 'right') => invoke<EditFile>('file_edit_open', { id, generation, fileId, side }),
  editClose: (ticket: string) => invoke<boolean>('file_edit_close', { ticket }),
  fileSave: (ticket: string, bytes: number[]) => invoke<RecoveryRecord>('file_save', { ticket, bytes }),
  recoveryList: () => invoke<RecoveryRecord[]>('recovery_list'),
  recoveryUndo: (id: string) => invoke<RecoveryRecord>('recovery_undo', { id }),
  recoveryCleanup: (ids: string[], confirmed: boolean) => invoke<number>('recovery_cleanup', { ids, confirmed }),
  recoveryResolve: (id: string, confirmed: boolean) => invoke<RecoveryRecord>('recovery_resolve', { id, confirmed }),
  loadSettings: () => invoke<{ sources: Source[]; workspace: Partial<Workspace> | null }>('load_settings'),
  saveSettings: (settings: { sources: Source[]; workspace: Workspace }) => invoke<void>('save_settings', { settings }),
  sourceRevision: (sourceId: string) => invoke<number>('source_revision', { sourceId }),
  credentialStatus: (sourceId: string) => invoke<CredentialStatus>('credential_status', { sourceId }),
  setToken: (sourceId: string, token: string) => invoke<void>('set_token', { sourceId, token }),
  hasToken: (sourceId: string) => invoke<boolean>('has_token', { sourceId }),
  deleteToken: (sourceId: string) => invoke<void>('delete_token', { sourceId }),
  testSource: (source: Source) => invoke<string>('test_source', { source }),
  listUserOrgs: (source: Source) => invoke<string[]>('list_user_orgs', { source }),
  listRepos: (source: Source, refresh: boolean) => invoke<RepoList>('list_repos', { source, refresh }),
  listCachedRepos: (source: Source) => invoke<RepoList | null>('list_cached_repos', { source }),
  getCommits: (source: Source, org: string, name: string, branch: string) =>
    invoke<Commit[]>('get_commits', { source, org, name, branch }),
  getRefsMany: (urls: string[]) => invoke<RefsResult[]>('get_refs_many', { urls }),
  pathsExist: (paths: string[]) => invoke<boolean[]>('paths_exist', { paths }),
  startClone: (jobs: CloneJob[], opts: CloneOpts, mode: GitAction = 'clone') => invoke<void>('start_clone', { jobs, opts, mode }),
  localStatus: (paths: string[]) => invoke<LocalStatus[]>('local_status', { paths }),
  activitySnapshot: () => invoke<Activity[]>('activity_snapshot'),
  clearActivity: () => invoke<{ running: Activity[]; retained: string[]; through: number }>('clear_activity'),
  cancelActivity: (id: string) => invoke<boolean>('cancel_activity', { id }),
  repositoryTree: (path: string) => invoke<RepositoryTree>('repository_tree', { path }),
  repositoryHistory: (path: string, limit?: number) => invoke<RepositoryHistory>('repository_history', { path, limit }),
  repoChanges: (path: string) => invoke<RepoChanges>('repo_changes', { path }),
  changeContent: (path: string, file: string, origPath: string | null, area: DiffArea) => invoke<ChangeContent>('change_content', { path, file, origPath, area }),
  stagePaths: (path: string, files: string[]) => invoke<void>('stage_paths', { path, files }),
  unstagePaths: (path: string, files: string[]) => invoke<void>('unstage_paths', { path, files }),
  commitStaged: (path: string, message: string) => invoke<{ sha: string; subject: string }>('commit_staged', { path, message }),
  createBranch: (path: string, name: string, start: string | null, switchTo: boolean) => invoke<void>('create_branch', { path, name, start, switch: switchTo }),
  pushBranch: (path: string) => invoke<{ remote: string; branch: string; upstreamSet: boolean }>('push_branch', { path }),
  deleteBranch: (path: string, name: string, force: boolean) => invoke<{ sha: string }>('delete_branch', { path, name, force }),
  trashSetFolders: (setId: string) => invoke<TrashOutcome[]>('trash_set_folders', { setId }),
  openComparison: (left: CompareEndpoint, right: CompareEndpoint) =>
    invoke<{ id: string; generation: number }>('comparison_open', { left, right }),
  refreshComparison: (id: string, options: CompareOptions) => invoke<CompareResult>('comparison_refresh', { id, options }),
  closeComparison: (id: string) => invoke<boolean>('comparison_close', { id }),
  cancelComparison: (id: string) => invoke<boolean>('comparison_cancel', { id }),
  comparisonFiles: (id: string, generation: number, offset = 0, limit = 512) =>
    invoke<CompareFile[]>('comparison_files', { id, generation, offset, limit }),
  comparisonContent: (id: string, generation: number, fileId: string, side: 'left' | 'right') =>
    invoke<CompareContent>('comparison_content', { id, generation, fileId, side }),
  comparisonCommits: (id: string, generation: number, offset = 0, limit = 200) =>
    invoke<CompareCommit[]>('comparison_commits', { id, generation, offset, limit }),
  openInVscode: (path: string) => invoke<void>('open_in_vscode', { path }),
  launchRequest: () => invoke<LaunchRequest[]>('launch_request'),
  discoverStart: (path: string, maxDepth?: number) => invoke<number>('discover_start', { path, maxDepth: maxDepth ?? null }),
  discoverCancel: (id: number) => invoke<boolean>('discover_cancel', { id }),
  discoverCancelAll: () => invoke<number>('discover_cancel_all'),
};
