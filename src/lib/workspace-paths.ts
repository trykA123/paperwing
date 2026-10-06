import type { PathIdentity, PlatformInfo, SetItem, Source, Workspace } from './api';
import { DEFAULT_TEMPLATE } from './workspace';

export function folderOf(item: SetItem) {
  return item.folder || item.name;
}

export function segments(workspace: Workspace, sources: Source[], item: SetItem, setName: string, platform: PlatformInfo['platform'] = 'windows'): string[] {
  const folder = folderOf(item);
  if (workspace.layout !== 'custom') return [folder];
  const flat = (s: string) => platform === 'linux' ? s.replace(/\/+/g, '-') : s.replace(/[\\/]+/g, '-');
  const source = sources.find(s => s.id === item.repoId.split(':')[0])?.name ?? '';
  const values: Record<string, string> = {
    folder, repo: item.name, org: item.org, set: flat(setName), source: flat(source),
    ref: flat(item.ref.type === 'commit' ? item.ref.name.slice(0, 8) : item.ref.name),
  };
  let tpl = workspace.pathTemplate.trim() || DEFAULT_TEMPLATE;
  // Without a per-repo token every row would land in the same folder.
  if (!/\{(folder|repo)\}/.test(tpl)) tpl += '\\{folder}';
  if (platform === 'linux') return tpl.split(/[\\/]+/).map(s => s.trim())
    .map(s => s.replace(/\{(\w+)\}/g, (match, key: string) => values[key] ?? match))
    .filter(s => s && s !== '.' && s !== '..');
  return tpl
    .replace(/\{(\w+)\}/g, (m, k: string) => values[k] ?? m)
    .split(/[\\/]+/)
    .map(s => s.replace(/[:*?"<>|\x00-\x1f]/g, '').trim().replace(/[. ]+$/, ''))
    .filter(s => s && s !== '.' && s !== '..');
}

export function destination(workspace: Workspace, sources: Source[], item: SetItem, setName: string, platform: PlatformInfo['platform'] = 'windows') {
  if (item.path) return item.path;
  const sep = platform === 'linux' ? '/' : '\\';
  const root = platform === 'linux' ? workspace.root.replace(/\/+$/, '') : workspace.root.replace(/[\\/]+$/, '');
  return [root, ...segments(workspace, sources, item, setName, platform)].join(sep);
}

export function collisionKey(path: string, platform: PlatformInfo['platform'] = 'windows', identities: Record<string, PathIdentity> = {}) {
  const physical = identities[path]?.identity;
  return physical ? `physical:${physical}` : `candidate:${platform === 'windows' ? path.toLowerCase() : path}`;
}

export function pathClashes(destinations: string[], platform: PlatformInfo['platform'] = 'windows', identities: Record<string, PathIdentity> = {}) {
  const result = new Map<string, number>();
  for (const destination of destinations) {
    const key = collisionKey(destination, platform, identities);
    result.set(key, (result.get(key) ?? 0) + 1);
  }
  return result;
}

export function uniqueFolder(base: string, items: SetItem[], platform: PlatformInfo['platform'] = 'windows') {
  const key = (name: string) => platform === 'windows' ? name.toLowerCase() : name;
  const taken = new Set(items.map(i => key(folderOf(i))));
  if (!taken.has(key(base))) return base;
  let n = 2;
  while (taken.has(key(`${base}_${n}`))) n++;
  return `${base}_${n}`;
}
