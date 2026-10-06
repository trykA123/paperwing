import { api, type CredentialStatus } from '../api';

export function credentialStoreName(backend: string) {
  return backend === 'windowsCredentialManager' ? 'Windows Credential Manager'
    : backend === 'secretService' ? 'your desktop credential store' : 'the credential store';
}

export function credentialLabel(status?: CredentialStatus) {
  if (!status) return 'checking token';
  return status.state === 'saved' ? 'token saved' : status.state === 'missing' ? 'no token'
    : status.state === 'locked' ? 'credential store locked'
    : status.state === 'unavailable' ? 'credential service unavailable'
    : status.state === 'permissionDenied' ? 'credential access denied'
    : status.state === 'uncertain' ? 'token outcome uncertain' : 'credential error';
}

export class Credentials {
  statuses = $state<Record<string, CredentialStatus>>({});
  private generations = new Map<string, number>();
  private revisions = new Map<string, number>();

  constructor(private invalidateMetadata: (sourceId: string) => void) {}

  invalidate(sourceId: string, revision?: number) {
    if (revision !== undefined) {
      if (revision <= (this.revisions.get(sourceId) ?? -1)) return;
      this.revisions.set(sourceId, revision);
    }
    this.generations.set(sourceId, (this.generations.get(sourceId) ?? 0) + 1);
    delete this.statuses[sourceId];
    this.invalidateMetadata(sourceId);
  }

  async refresh(sourceId: string) {
    const generation = (this.generations.get(sourceId) ?? 0) + 1;
    this.generations.set(sourceId, generation);
    try {
      const status = await api.credentialStatus(sourceId);
      if (this.generations.get(sourceId) === generation
        && status.revision >= (this.revisions.get(sourceId) ?? 0)) {
        const revisionChanged = status.revision > (this.revisions.get(sourceId) ?? 0);
        if (revisionChanged) this.invalidate(sourceId, status.revision);
        else if (status.state !== 'saved' && status.state !== 'missing') this.invalidateMetadata(sourceId);
        this.revisions.set(sourceId, status.revision);
        this.statuses[sourceId] = status;
      }
    } catch (error) {
      if (this.generations.get(sourceId) === generation) {
        const previous = this.statuses[sourceId];
        this.invalidateMetadata(sourceId);
        this.statuses[sourceId] = { sourceId, backend: previous?.backend ?? 'unsupported',
          state: previous?.state === 'uncertain' ? 'uncertain' : 'error',
          revision: this.revisions.get(sourceId) ?? 0, reason: String(error) };
      }
    }
  }

  async synchronize(sourceId: string) {
    this.invalidate(sourceId, await api.sourceRevision(sourceId));
  }

  async mutate(sourceId: string, token?: string) {
    this.invalidate(sourceId);
    try {
      if (token === undefined) await api.deleteToken(sourceId);
      else await api.setToken(sourceId, token);
    } finally {
      await this.refresh(sourceId);
    }
  }
}
