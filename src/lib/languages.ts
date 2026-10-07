import type { Extension } from '@codemirror/state';

export type LanguageEntry = { id: string; label: string; extensions: readonly string[]; load: () => Promise<Extension> };

export const PLAIN_TEXT = 'plaintext';

export const LANGUAGES: readonly LanguageEntry[] = [
  { id: 'cpp', label: 'C and C++', extensions: ['c', 'h', 'cpp', 'hpp'], load: async () => (await import('@codemirror/lang-cpp')).cpp() },
  { id: 'rust', label: 'Rust', extensions: ['rs'], load: async () => (await import('@codemirror/lang-rust')).rust() },
  { id: 'python', label: 'Python', extensions: ['py'], load: async () => (await import('@codemirror/lang-python')).python() },
  { id: 'javascript', label: 'JavaScript', extensions: ['js', 'jsx'], load: async () => (await import('@codemirror/lang-javascript')).javascript({ jsx: true }) },
  { id: 'typescript', label: 'TypeScript', extensions: ['ts', 'tsx'], load: async () => (await import('@codemirror/lang-javascript')).javascript({ jsx: true, typescript: true }) },
  { id: 'xml', label: 'XML', extensions: ['xml', 'arxml'], load: async () => (await import('@codemirror/lang-xml')).xml() },
  { id: 'yaml', label: 'YAML', extensions: ['yml', 'yaml'], load: async () => (await import('@codemirror/lang-yaml')).yaml() },
];

export function languageId(path: string): string {
  const extension = path.split('.').at(-1)?.toLowerCase() ?? '';
  return LANGUAGES.find(entry => entry.extensions.includes(extension))?.id ?? PLAIN_TEXT;
}

export function findLanguage(id: string): LanguageEntry | undefined {
  return LANGUAGES.find(entry => entry.id === id);
}
