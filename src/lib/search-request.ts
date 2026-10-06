import type { SearchMode, SearchRequest } from './api';

export type SearchForm = {
  pattern: string; mode: SearchMode; ignoreCase: boolean; wholeWord: boolean; pathspecs: string; context: number; untracked: boolean;
};
export type SearchTarget = { path: string; name: string; gitRef: string };

export const defaultSearchForm = (): SearchForm => ({ pattern: '', mode: 'fixed', ignoreCase: true, wholeWord: false, pathspecs: '', context: 0, untracked: false });

export function parsePathspecs(text: string): string[] {
  return text.split(/[\n,]/).map(spec => spec.trim()).filter(Boolean);
}

export function buildSearchRequest(targets: readonly SearchTarget[], form: SearchForm): SearchRequest {
  const refs = targets.some(target => target.gitRef.trim());
  return {
    repos: targets.map(target => ({ path: target.path, gitRef: target.gitRef.trim() || null })),
    pattern: form.pattern, mode: form.mode, ignoreCase: form.ignoreCase, wholeWord: form.wholeWord,
    pathspecs: parsePathspecs(form.pathspecs), context: form.context, untracked: form.untracked && !refs,
  };
}

export const isSearchLimitError = (message: string): boolean => /too many searches/i.test(message);
