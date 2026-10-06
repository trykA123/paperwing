import { api, type RootSupport } from '../api';
import { unavailableRoot } from '../platform';

export class RootProbes {
  probes = $state<Record<string, RootSupport>>({});
  private revisions = new Map<string, number>();
  private requests = new Map<string, Promise<RootSupport>>();
  private completed = new Map<string, string>();

  constructor(private onIdentityChange: () => void) {}

  identity(root: string) { return this.completed.get(root) ?? ''; }

  async probe(root: string) {
    const previousIdentity = this.completed.get(root);
    const current = (this.revisions.get(root) ?? 0) + 1;
    this.revisions.set(root, current);
    this.probes[root] = unavailableRoot(root, 'Checking native root support.');
    const request = (async () => {
      let support: RootSupport;
      try { support = await api.probeRoot(root); }
      catch (reason) { support = unavailableRoot(root, String(reason)); }
      if (this.revisions.get(root) !== current) return this.requests.get(root)
        ?? this.probes[root] ?? unavailableRoot(root, 'Root support changed. Try again.');
      if (previousIdentity && (support.identity ?? '') !== previousIdentity) this.onIdentityChange();
      this.completed.set(root, support.identity ?? '');
      this.probes[root] = support;
      return support;
    })();
    this.requests.set(root, request);
    try { return await request; }
    finally { if (this.requests.get(root) === request) this.requests.delete(root); }
  }
}
