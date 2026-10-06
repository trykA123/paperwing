export const supportedCapability = { supported: true, reason: null };
export const windowsCapabilities = Object.fromEntries(['readCompare', 'edit', 'copy', 'recovery', 'trash'].map(name => [name, supportedCapability]));
export const windowsPlatform = { platform: 'windows', separator: '\\', capabilities: windowsCapabilities };
export const linuxCapabilities = { ...windowsCapabilities, ...Object.fromEntries(['edit', 'copy', 'trash'].map(name => [name, {
    supported: false, reason: `Linux ${name} backend is unavailable.`,
}])) };
export const linuxPlatform = { platform: 'linux', separator: '/', capabilities: linuxCapabilities };
export const supportedRoot = (root, capabilities = windowsCapabilities) => ({ root, valid: true, reason: null, identity: `fixture:${root}`, casePolicy: 'unknown', capabilities });
