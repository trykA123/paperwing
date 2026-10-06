import type { CompareEndpoint, PlatformInfo, Workspace } from './api';

export type View =
  | { kind: 'set' }
  | { kind: 'item'; itemId: string }
  | { kind: 'org'; source: string; org: string }
  | { kind: 'search' }
  | { kind: 'compare'; comparisonId: string; left: CompareEndpoint; right: CompareEndpoint; readOnly?: boolean }
  | { kind: 'setCompare'; comparisonId: string }
  | { kind: 'fileDiff'; comparisonId: string; fileId: string; path: string; sessionId: string; generation: number }
  | { kind: 'settings' };
export type ShellTab = { id: string; setId: string; view: View; query: string; page: number; filter: string };

export function tabId(view: View, setId: string): string {
  switch (view.kind) {
    case 'set': return `set:${setId}`;
    case 'item': return `item:${setId}:${view.itemId}`;
    case 'org': return `org:${setId}:${view.source}:${view.org}`;
    case 'search': return `search:${setId}`;
    case 'compare': return `compare:${view.comparisonId}`;
    case 'setCompare': return `setCompare:${view.comparisonId}`;
    case 'fileDiff': return `fileDiff:${view.comparisonId}:${view.fileId}`;
    case 'settings': return 'settings';
  }
}

export const DEFAULT_COLS = { repo: 210, checkout: 190, local: 220, status: 170 };
export const DEFAULT_TEMPLATE = '{org}\\{folder}';

export function defaultWorkspace(platform: PlatformInfo['platform'] = 'windows'): Workspace {
  const id = Math.random().toString(36).slice(2, 10);
  return {
    sets: [{ id, name: 'My first set', items: [] }], stars: [], activeSet: id,
    root: platform === 'windows' ? 'C:\\Dev\\repos' : '', layout: 'flat', pathTemplate: DEFAULT_TEMPLATE, cols: { ...DEFAULT_COLS },
    shallow: false, parallel: 4, onExisting: 'fetch', pageSize: 25, rightWidth: 380,
    theme: 'system', uiFont: 'geist', codeFont: 'geist-mono',
    shell: { version: 1, sidebarWidth: 250, sidebarVisible: true, rightVisible: true },
  };
}

export function migrateWorkspace(saved: Partial<Workspace> | null, platform: PlatformInfo['platform'] = 'windows'): Workspace {
  const defaults = defaultWorkspace(platform);
  const ws = { ...defaults, ...saved };
  if (!ws.sets.length) ws.sets = defaults.sets;
  if (!ws.sets.some(set => set.id === ws.activeSet)) ws.activeSet = ws.sets[0].id;
  if ((ws.layout as string) === 'org') {
    ws.layout = 'custom';
    ws.pathTemplate = DEFAULT_TEMPLATE;
  }
  ws.cols = { ...defaults.cols, ...ws.cols };
  ws.shell = { ...defaults.shell, ...ws.shell };
  ws.shell.sidebarWidth = Math.round(Math.max(190, Math.min(360, ws.shell.sidebarWidth)));
  return ws;
}