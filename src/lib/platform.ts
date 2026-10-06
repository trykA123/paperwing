import type { Capability, Capabilities, PlatformInfo, RootSupport } from './api';

export const pendingCapability: Capability = { supported: false, reason: 'Platform support is loading.' };
export const pendingCapabilities: Capabilities = {
  readCompare: pendingCapability, edit: pendingCapability, copy: pendingCapability,
  recovery: pendingCapability, trash: pendingCapability,
};
export const pendingPlatform: PlatformInfo = { platform: 'unsupported', separator: '/', capabilities: pendingCapabilities, credentials: { backend: 'unsupported', persistent: false, supported: false, reason: 'Credential support is loading.' } };

export function unavailableRoot(root: string, reason: string): RootSupport {
  const unavailable = { supported: false, reason };
  return { root, valid: false, reason, identity: null, casePolicy: 'unknown', capabilities: {
    readCompare: unavailable, edit: unavailable, copy: unavailable, recovery: unavailable, trash: unavailable,
  } };
}
