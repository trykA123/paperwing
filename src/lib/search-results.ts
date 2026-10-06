import type { SearchMatch, SearchRepoStatus } from './api';

export const SEARCH_ROW_HEIGHT = 24;
export type SearchGroup = { repo: string; name: string; matches: SearchMatch[]; status: SearchRepoStatus | null };
export type SearchRow =
  | { kind: 'repo'; key: string; name: string; status: SearchRepoStatus | null; count: number }
  | { kind: 'file'; key: string; path: string; count: number }
  | { kind: 'context'; key: string; line: number; text: string }
  | { kind: 'match'; key: string; repo: string; match: SearchMatch };

type Line = { line: number; order: number; row: SearchRow };

function fileLines(repo: string, path: string, matches: SearchMatch[]): SearchRow[] {
  const matched = new Set(matches.map(match => match.line));
  const lines: Line[] = [];
  const seen = new Set<number>();
  matches.forEach((match, index) => {
    const key = `m:${repo}:${path}:${match.line}:${match.column}:${index}`;
    lines.push({ line: match.line, order: 1, row: { kind: 'match', key, repo, match } });
    for (const context of match.context) {
      if (matched.has(context.line) || seen.has(context.line)) continue;
      seen.add(context.line);
      lines.push({ line: context.line, order: 0, row: { kind: 'context', key: `c:${repo}:${path}:${context.line}`, line: context.line, text: context.text } });
    }
  });
  return lines.sort((a, b) => a.line - b.line || a.order - b.order).map(entry => entry.row);
}

/** Flattens repository, file and line groups into fixed-height rows for the virtual list. */
export function buildRows(groups: readonly SearchGroup[]): SearchRow[] {
  const rows: SearchRow[] = [];
  for (const group of groups) {
    if (!group.matches.length && !group.status) continue;
    rows.push({ kind: 'repo', key: `r:${group.repo}`, name: group.name, status: group.status, count: group.matches.length });
    const files = new Map<string, SearchMatch[]>();
    for (const match of group.matches) {
      const list = files.get(match.path);
      if (list) list.push(match); else files.set(match.path, [match]);
    }
    for (const [path, matches] of files) {
      rows.push({ kind: 'file', key: `f:${group.repo}:${path}`, path, count: matches.length });
      rows.push(...fileLines(group.repo, path, matches));
    }
  }
  return rows;
}

export type SearchQuery = { pattern: string; mode?: 'fixed' | 'basic' | 'perl'; ignoreCase?: boolean };
const ENCODER = new TextEncoder();
const DECODER = new TextDecoder();

/** Git reports the column in bytes; returns the matched span in string indices, or null when it cannot be found. */
export function highlightRange(match: SearchMatch, query: SearchQuery): [number, number] | null {
  const start = DECODER.decode(ENCODER.encode(match.text).slice(0, Math.max(0, match.column - 1))).length;
  if (start >= match.text.length) return null;
  let length = query.pattern.length;
  if (query.mode && query.mode !== 'fixed') {
    try {
      const expression = new RegExp(query.pattern, query.ignoreCase ? 'iy' : 'y');
      expression.lastIndex = start;
      length = expression.exec(match.text)?.[0].length ?? 0;
    } catch { return null; }
  }
  return length > 0 ? [start, Math.min(match.text.length, start + length)] : null;
}

export function describeRepoStatus(status: SearchRepoStatus | null): { label: string; tone: 'ok' | 'warn' | 'err' | 'mut' } {
  if (!status) return { label: 'searching', tone: 'mut' };
  if (status.state === 'failed') return { label: 'failed', tone: 'err' };
  if (status.state === 'cancelled') return { label: 'cancelled', tone: 'warn' };
  if (status.state === 'skipped') return { label: 'skipped', tone: 'warn' };
  return status.truncated ? { label: 'truncated', tone: 'warn' } : { label: 'done', tone: 'ok' };
}

export function matchLocation(repoPath: string, match: SearchMatch): string {
  const separator = repoPath.includes('\\') && !repoPath.includes('/') ? '\\' : '/';
  const base = repoPath.replace(/[\\/]+$/, '');
  return `${base}${separator}${separator === '\\' ? match.path.replaceAll('/', '\\') : match.path}`;
}
