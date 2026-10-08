import type { IconName } from '../components/Icon.svelte';
import type { RailSection } from './api';
import type { View } from './workspace';

export type ModuleId = RailSection | 'settings';
export type ModuleGroup = 'local' | 'provider' | 'system';
export type ProviderId = 'github' | 'jira';
export type RailBadge = { count: number; tone?: 'err' | 'acc'; label?: string };
export type BadgeContext = { comparisons: number; awaitingReview: number; failedRuns: number; gitFailed: number; gitRunning: number };
export type ModuleDef = {
  id: ModuleId; label: string; icon: IconName; group: ModuleGroup; provider?: ProviderId;
  itemLabel?: string; shortcutKey?: string; badge?: (context: BadgeContext) => RailBadge | undefined;
};
export type ProviderDef = { id: ProviderId; label: string; icon: IconName; kinds: readonly string[]; defaultHost?: string };
export type ProviderHost = { host: string; sourceIds: string[] };
export type RailItem = { module: ModuleDef; badge?: RailBadge };
export type ProviderEntry = { provider: ProviderDef; hosts: ProviderHost[]; items: RailItem[]; badge?: RailBadge };
export type RailLayout = { local: RailItem[]; providers: ProviderEntry[]; system: RailItem[] };

const count = (value: number, label: string, tone?: RailBadge['tone']): RailBadge | undefined => (value > 0 ? { count: value, tone, label } : undefined);

export const MODULES: readonly ModuleDef[] = [
  { id: 'repos', label: 'Repositories', icon: 'repo', group: 'local', shortcutKey: '1' },
  { id: 'changes', label: 'Changes', icon: 'changes', group: 'local', shortcutKey: '2' },
  { id: 'branches', label: 'Branches & tags', icon: 'branch', group: 'local', shortcutKey: '3' },
  { id: 'compare', label: 'Compare', icon: 'copy', group: 'local', shortcutKey: '4', badge: context => count(context.comparisons, 'open') },
  { id: 'search', label: 'Search', icon: 'search', group: 'local', shortcutKey: '5' },
  { id: 'prs', label: 'Pull requests', icon: 'pr', group: 'provider', provider: 'github', badge: context => count(context.awaitingReview, 'need review') },
  { id: 'actions', label: 'Actions', icon: 'actions', group: 'provider', provider: 'github', badge: context => count(context.failedRuns, 'failed', 'err') },
  { id: 'releases', label: 'Releases', icon: 'tag', group: 'provider', provider: 'github' },
  { id: 'jira', label: 'Jira', itemLabel: 'Issues', icon: 'jira', group: 'provider', provider: 'jira' },
  { id: 'activity', label: 'Activity', icon: 'activity', group: 'system', shortcutKey: 'j',
    badge: context => (context.gitFailed ? count(context.gitFailed, 'failed', 'err') : count(context.gitRunning, 'running')) },
  { id: 'recovery', label: 'Recovery', icon: 'undo', group: 'system' },
  { id: 'settings', label: 'Settings', icon: 'gear', group: 'system', shortcutKey: ',' },
];

export const PROVIDERS: readonly ProviderDef[] = [
  { id: 'github', label: 'GitHub', icon: 'server', kinds: ['github', 'ghe'], defaultHost: 'github.com' },
  { id: 'jira', label: 'Jira', icon: 'board', kinds: ['jira'] },
];

/** The module a fresh or unreadable workspace opens; the registry order and this constant are the only places that name it. */
export const HOME_MODULE: RailSection = 'repos';
export const SECTIONS: readonly RailSection[] = MODULES.filter(module => module.id !== 'settings').map(module => module.id as RailSection);
export const isSection = (value: unknown): value is RailSection => SECTIONS.includes(value as RailSection);
export const moduleById = (id: ModuleId): ModuleDef => MODULES.find(module => module.id === id)!;
export const providerOf = (module: ModuleDef): ProviderDef | undefined => PROVIDERS.find(provider => provider.id === module.provider);
export const shortcutLabel = (module: ModuleDef) => (module.shortcutKey ? `Ctrl+${module.shortcutKey.toUpperCase()}` : undefined);

type SourceFacts = { id: string; kind: string; host: string };
const sourcesOf = (provider: ProviderDef, sources: readonly SourceFacts[]) => sources.filter(source => provider.kinds.includes(source.kind));

/** One entry per distinct host; the provider's default host comes first. */
export function providerHosts(provider: ProviderDef, sources: readonly SourceFacts[]): ProviderHost[] {
  const hosts = new Map<string, ProviderHost>();
  for (const source of sourcesOf(provider, sources)) {
    const host = source.host.trim().toLowerCase() || provider.defaultHost || source.id;
    const known = hosts.get(host) ?? { host, sourceIds: [] };
    known.sourceIds.push(source.id);
    hosts.set(host, known);
  }
  return [...hosts.values()].sort((a, b) => Number(b.host === provider.defaultHost) - Number(a.host === provider.defaultHost));
}

export const visibleProviders = (sources: readonly SourceFacts[]) => PROVIDERS.filter(provider => sourcesOf(provider, sources).length > 0);

export function isModuleVisible(id: ModuleId, sources: readonly SourceFacts[]): boolean {
  const module = moduleById(id);
  return !module.provider || visibleProviders(sources).some(provider => provider.id === module.provider);
}

/** A provider button sums its modules' badges and turns red when any of them is red. */
export function providerBadge(items: readonly RailItem[]): RailBadge | undefined {
  const badges = items.flatMap(item => item.badge ?? []);
  const total = badges.reduce((sum, badge) => sum + badge.count, 0);
  const label = [...new Set(badges.map(badge => badge.label).filter(Boolean))].join(' and ');
  return total ? { count: total, tone: badges.some(badge => badge.tone === 'err') ? 'err' : 'acc', label } : undefined;
}

export function railLayout(sources: readonly SourceFacts[], context: BadgeContext): RailLayout {
  const item = (module: ModuleDef): RailItem => ({ module, badge: module.badge?.(context) });
  const inGroup = (group: ModuleGroup) => MODULES.filter(module => module.group === group && !module.provider).map(item);
  const providers = visibleProviders(sources).map(provider => {
    const items = MODULES.filter(module => module.provider === provider.id).map(item);
    return { provider, hosts: providerHosts(provider, sources), items, badge: providerBadge(items) };
  });
  return { local: inGroup('local'), providers, system: inGroup('system') };
}

/** Ctrl+1..5 pick the Local Git modules, Ctrl+J opens Activity and Ctrl+, opens Settings. */
export function moduleShortcut(key: string): ModuleId | undefined {
  const lower = key.toLowerCase();
  return MODULES.find(module => module.shortcutKey === lower)?.id;
}

const REPO_VIEWS: readonly View['kind'][] = ['repos', 'repo', 'set', 'item', 'org', 'search'];
const COMPARE_VIEWS: readonly View['kind'][] = ['compare', 'setCompare', 'fileDiff'];

export function moduleOfView(view: View): RailSection | undefined {
  if (view.kind === 'module') return view.module;
  if (view.kind === 'codeSearch') return 'search';
  if (REPO_VIEWS.includes(view.kind)) return 'repos';
  return COMPARE_VIEWS.includes(view.kind) ? 'compare' : undefined;
}

const PAGE_MODULES: readonly ModuleId[] = ['changes', 'branches', 'prs', 'actions', 'releases', 'jira'];
const OWN_VIEWS: Partial<Record<ModuleId, View>> = { repos: { kind: 'repos' }, settings: { kind: 'settings' } };

/** The main-area view a module opens; Search, Compare, Activity and Recovery open theirs through their own flows. */
export function viewOfModule(id: ModuleId): View | undefined {
  return OWN_VIEWS[id] ?? (PAGE_MODULES.includes(id) ? { kind: 'module', module: id as RailSection } : undefined);
}

/** Search and every module with a tab own a page; Activity, Recovery and Compare are sidebar-only. */
export const ownsPage = (id: ModuleId) => id === 'search' || !!viewOfModule(id);

/** Opening a tab moves the sidebar to its module only when the sidebar is showing a page module. */
export function sectionOnActivate(current: RailSection, tabModule: RailSection | undefined): RailSection {
  return tabModule && ownsPage(current) ? tabModule : current;
}
