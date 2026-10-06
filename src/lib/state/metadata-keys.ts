import type { Source } from '../api';

export function sourceFingerprint(source: Source) {
  return JSON.stringify([source.kind, source.host ?? '', source.orgs ?? [], source.urls ?? [], source.credentialManaged ?? false]);
}

export function commitHistoryKey(scope: { source: string; repository: string; branch: string; refEpoch: number }) {
  return JSON.stringify(['commits-v2', scope.source, scope.repository, scope.branch, scope.refEpoch]);
}
