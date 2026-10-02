import type { CompareFile, CompareRef, CompareStatus } from './api';
import type { View } from './workspace';

export const statusLabel: Record<CompareStatus, string> = {
  same: 'Identical', different: 'Different', leftOnly: 'Only on the left', rightOnly: 'Only on the right',
  typeConflict: 'Type conflict', unavailable: 'Unavailable',
};
export const statusMark: Record<CompareStatus, string> = {
  same: '=', different: '\u2260', leftOnly: '\u25c2', rightOnly: '\u25b8', typeConflict: '!', unavailable: '?',
};
export const expandable = (file: CompareFile) => file.left?.kind === 'directory' || file.right?.kind === 'directory';
export const directory = (file: CompareFile) => expandable(file) && [file.left, file.right].every(side => !side || side.kind === 'directory');
export function detailFile(view: View, comparisonId: string, files: CompareFile[], selectedId: string | null, snapshot: { id: string; generation: number } | null) {
  if (view.kind !== 'fileDiff') return files.find(file => file.id === selectedId);
  if (view.comparisonId !== comparisonId || !snapshot || view.sessionId !== snapshot.id || view.generation !== snapshot.generation) return undefined;
  return files.find(file => file.id === view.fileId);
}
export function refLabel(reference: CompareRef) {
  return reference.kind === 'workingTree' ? 'Working tree' : reference.kind === 'head' ? 'HEAD'
    : reference.kind === 'commit' ? reference.sha : 'name' in reference ? reference.name : '';
}
export function pathMatches(path: string, pattern: string): boolean {
  pattern = pattern.trim().replaceAll('\\', '/');
  if (!pattern) return true;
  if (pattern.endsWith('/')) return (`/${path}/`).toLowerCase().includes(`/${pattern}`.toLowerCase());
  if (!/[?*]/.test(pattern)) return path.toLowerCase().includes(pattern.toLowerCase());
  let expression = '';
  for (let index = 0; index < pattern.length; index++) {
    const character = pattern[index];
    if (character === '*' && pattern[index + 1] === '*') {
      index++;
      if (pattern[index + 1] === '/') { index++; expression += '(?:.*/)?'; }
      else expression += '.*';
    } else if (character === '*') expression += '[^/]*';
    else if (character === '?') expression += '[^/]';
    else expression += character.replace(/[\\^$.*+?()[\]{}|]/g, '\\$&');
  }
  return new RegExp(`^${expression}$`, 'i').test(pattern.includes('/') ? path : path.split('/').at(-1)!);
}
export function compareRows(files: CompareFile[], filter: string, query: string, excludes: string, expanded: string[]) {
  const excluded = excludes.split(';').map(value => value.trim()).filter(Boolean);
  const visible = files.filter(file => !directory(file) && pathMatches(file.path, query)
    && !excluded.some(pattern => pathMatches(file.path, pattern))
    && (filter === 'all' || (filter === 'differences' && file.displayStatus !== 'same')
      || (filter === 'same' && file.displayStatus === 'same')
      || (filter === 'orphans' && ['leftOnly', 'rightOnly'].includes(file.displayStatus))));
  const ancestors = new Set<string>();
  for (const file of visible) {
    const parts = file.path.split('/');
    for (let length = 1; length < parts.length; length++) ancestors.add(parts.slice(0, length).join('/'));
  }
  const visibleIds = new Set(visible.map(file => file.id));
  const ordered = files.filter(file => ancestors.has(file.path) || (!directory(file) && visibleIds.has(file.id)));
  ordered.sort((left, right) => {
    const leftParts = left.path.split('/'), rightParts = right.path.split('/');
    for (let index = 0; index < Math.min(leftParts.length, rightParts.length); index++) {
      if (leftParts[index] === rightParts[index]) continue;
      const leftFolder = index < leftParts.length - 1 || directory(left);
      const rightFolder = index < rightParts.length - 1 || directory(right);
      if (leftFolder !== rightFolder) return leftFolder ? -1 : 1;
      return leftParts[index].localeCompare(rightParts[index]);
    }
    return leftParts.length - rightParts.length;
  });
  return ordered.filter(file => {
    const parts = file.path.split('/');
    return parts.slice(0, -1).every((_, index) => expanded.includes(parts.slice(0, index + 1).join('/')));
  });
}
export function sizeLabel(size: number | null | undefined) {
  if (size == null) return '\u2014';
  return size < 1024 ? `${size} B` : size < 1024 * 1024 ? `${(size / 1024).toFixed(1)} KB` : `${(size / (1024 * 1024)).toFixed(1)} MB`;
}