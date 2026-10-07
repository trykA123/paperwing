const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import { isModuleVisible, isSection, moduleOfView, moduleShortcut, providerHosts, PROVIDERS, railClick, railLayout, viewOfModule, visibleProviders, type BadgeContext } from './modules';
import { migrateWorkspace, tabId } from './workspace';
import type { Workspace } from './api';

const quiet: BadgeContext = { comparisons: 0, awaitingReview: 0, failedRuns: 0, gitFailed: 0, gitRunning: 0 };
const github = { id: 'g', kind: 'github' as const, host: 'github.com' };
const ghes = { id: 'e', kind: 'ghe' as const, host: 'git.acme.example' };
const manual = { id: 'm', kind: 'manual' as const, host: '' };
const jira = { id: 'j', kind: 'jira', host: 'jira.acme.example' };
const ids = (items: { module: { id: string } }[]) => items.map(item => item.module.id);

describe('rail layout', () => {
  test('groups the modules and shows no provider without a source', () => {
    const layout = railLayout([manual], quiet);
    expect(ids(layout.local)).toEqual(['sets', 'changes', 'branches', 'compare', 'search']);
    expect(layout.providers).toEqual([]);
    expect(ids(layout.system)).toEqual(['activity', 'recovery', 'settings']);
  });

  test('a GitHub or GHES source shows the GitHub button; Jira stays hidden until a Jira source exists', () => {
    expect(visibleProviders([ghes]).map(provider => provider.id)).toEqual(['github']);
    expect(visibleProviders([github, jira]).map(provider => provider.id)).toEqual(['github', 'jira']);
    expect(visibleProviders([jira]).map(provider => provider.id)).toEqual(['jira']);
    expect(visibleProviders([])).toEqual([]);
  });

  test('a hidden provider takes its modules with it', () => {
    expect(isModuleVisible('prs', [manual])).toBe(false);
    expect(isModuleVisible('jira', [github])).toBe(false);
    expect(isModuleVisible('actions', [ghes])).toBe(true);
    expect(isModuleVisible('sets', [])).toBe(true);
  });

  test('GitHub lists pull requests, actions and releases', () => {
    const [entry] = railLayout([github], quiet).providers;
    expect(ids(entry.items)).toEqual(['prs', 'actions', 'releases']);
  });
});

describe('provider hosts', () => {
  const [githubProvider] = PROVIDERS;

  test('one section per host, github.com first, sources on one host merged', () => {
    const second = { id: 'g2', kind: 'github' as const, host: 'GitHub.com' };
    expect(providerHosts(githubProvider, [ghes, github, second])).toEqual([
      { host: 'github.com', sourceIds: ['g', 'g2'] },
      { host: 'git.acme.example', sourceIds: ['e'] },
    ]);
  });

  test('a GitHub source without a host reads as github.com', () => {
    expect(providerHosts(githubProvider, [{ id: 'g', kind: 'github', host: '' }])).toEqual([{ host: 'github.com', sourceIds: ['g'] }]);
  });

  test('manual sources belong to no provider', () => {
    expect(providerHosts(githubProvider, [manual])).toEqual([]);
  });
});

describe('badges', () => {
  test('the provider badge sums its modules and stays blue', () => {
    const [entry] = railLayout([github], { ...quiet, awaitingReview: 4 }).providers;
    expect(entry.badge).toEqual({ count: 4, tone: 'acc' });
  });

  test('a failed run turns the provider badge red and adds to the sum', () => {
    const [entry] = railLayout([github], { ...quiet, awaitingReview: 4, failedRuns: 2 }).providers;
    expect(entry.badge).toEqual({ count: 6, tone: 'err' });
  });

  test('no count means no badge', () => {
    expect(railLayout([github], quiet).providers[0].badge).toBeUndefined();
    expect(railLayout([github], quiet).local.every(item => !item.badge)).toBe(true);
  });

  test('compare counts open comparisons; activity is red when a command failed, else counts running ones', () => {
    const busy = railLayout([], { ...quiet, comparisons: 2, gitRunning: 3 });
    expect(busy.local.find(item => item.module.id === 'compare')?.badge).toEqual({ count: 2, tone: undefined });
    expect(busy.system[0].badge).toEqual({ count: 3, tone: undefined });
    expect(railLayout([], { ...quiet, gitRunning: 3, gitFailed: 1 }).system[0].badge).toEqual({ count: 1, tone: 'err' });
  });
});

describe('shortcuts and clicks', () => {
  test('Ctrl+1 to Ctrl+5 pick the Local Git modules', () => {
    expect(['1', '2', '3', '4', '5'].map(moduleShortcut)).toEqual(['sets', 'changes', 'branches', 'compare', 'search']);
    expect(moduleShortcut('6')).toBeUndefined();
  });

  test('Ctrl+J opens Activity and Ctrl+comma opens Settings', () => {
    expect(moduleShortcut('j')).toBe('activity');
    expect(moduleShortcut('J')).toBe('activity');
    expect(moduleShortcut(',')).toBe('settings');
    expect(moduleShortcut('a')).toBeUndefined();
  });

  test('clicking the active module folds the sidebar and any other click shows its module', () => {
    expect(railClick({ section: 'sets', sidebarVisible: true }, 'sets')).toEqual({ section: 'sets', sidebarVisible: false });
    expect(railClick({ section: 'sets', sidebarVisible: false }, 'sets')).toEqual({ section: 'sets', sidebarVisible: true });
    expect(railClick({ section: 'sets', sidebarVisible: true }, 'activity')).toEqual({ section: 'activity', sidebarVisible: true });
  });
});

describe('views and workspaces', () => {
  test('a view belongs to one module', () => {
    expect(moduleOfView({ kind: 'set' })).toBe('sets');
    expect(moduleOfView({ kind: 'org', source: 's', org: 'o' })).toBe('sets');
    expect(moduleOfView({ kind: 'codeSearch' })).toBe('search');
    expect(moduleOfView({ kind: 'setCompare', comparisonId: 'c' })).toBe('compare');
    expect(moduleOfView({ kind: 'module', module: 'prs' })).toBe('prs');
    expect(moduleOfView({ kind: 'settings' })).toBeUndefined();
  });

  test('page modules open one tab each; Search, Compare, Activity and Recovery open none', () => {
    expect(viewOfModule('prs')).toEqual({ kind: 'module', module: 'prs' });
    expect(viewOfModule('sets')).toEqual({ kind: 'set' });
    expect(viewOfModule('settings')).toEqual({ kind: 'settings' });
    for (const id of ['search', 'compare', 'activity', 'recovery'] as const) expect(viewOfModule(id)).toBeUndefined();
    expect(tabId({ kind: 'module', module: 'prs' }, 'a')).toBe(tabId({ kind: 'module', module: 'prs' }, 'b'));
  });

  test('a saved section survives only when it is a module', () => {
    expect(isSection('prs')).toBe(true);
    expect(isSection('settings')).toBe(false);
    const saved = (section: string) => ({ shell: { version: 1, sidebarWidth: 250, sidebarVisible: true, rightVisible: true, section } }) as unknown as Partial<Workspace>;
    expect(migrateWorkspace(saved('actions')).shell.section).toBe('actions');
    expect(migrateWorkspace(saved('elsewhere')).shell.section).toBe('sets');
  });
});
