import type { StreamParser } from '@codemirror/language';
import type { Extension } from '@codemirror/state';

export type LanguageEntry = {
  id: string;
  label: string;
  extensions: readonly string[];
  names?: readonly string[];
  load: () => Promise<Extension>;
};
export type LanguageMap = Readonly<Record<string, string>>;

export const PLAIN_TEXT = 'plaintext';
export const PLAIN_LABEL = 'Plain text';

type Parsers<K extends string> = Record<K, unknown>;

const from = <K extends string>(module: () => Promise<Parsers<K>>, name: K) => async (): Promise<Extension> => {
  const [{ StreamLanguage }, parsers] = await Promise.all([import('@codemirror/language'), module()]);
  return StreamLanguage.define(parsers[name] as StreamParser<unknown>);
};

export const LANGUAGES: readonly LanguageEntry[] = [
  { id: 'cpp', label: 'C and C++', extensions: ['c', 'h', 'i', 'cpp', 'cc', 'cxx', 'hpp', 'hh', 'hxx', 'inl'], load: async () => (await import('@codemirror/lang-cpp')).cpp() },
  { id: 'xml', label: 'XML', load: async () => (await import('@codemirror/lang-xml')).xml(),
    extensions: ['xml', 'arxml', 'xsd', 'xsl', 'xslt', 'epc', 'xdm', 'cdd', 'odx', 'odx-d', 'odx-c', 'pdx', 'cdfx', 'fibex', 'vsysvar', 'svd', 'launch', 'cproject', 'project'] },
  { id: 'ini', label: 'INI', extensions: ['ini', 'cfg', 'conf', 'properties', 'prefs'], load: from(() => import('@codemirror/legacy-modes/mode/properties'), 'properties') },
  { id: 'json', label: 'JSON', extensions: ['json', 'jsonc', 'json5'], load: async () => (await import('@codemirror/lang-json')).json() },
  { id: 'yaml', label: 'YAML', extensions: ['yml', 'yaml'], load: async () => (await import('@codemirror/lang-yaml')).yaml() },
  { id: 'python', label: 'Python', extensions: ['py'], names: ['sconstruct', 'sconscript'], load: async () => (await import('@codemirror/lang-python')).python() },
  { id: 'shell', label: 'Shell', extensions: ['sh', 'bash'], load: from(() => import('@codemirror/legacy-modes/mode/shell'), 'shell') },
  { id: 'powershell', label: 'PowerShell', extensions: ['ps1'], load: from(() => import('@codemirror/legacy-modes/mode/powershell'), 'powerShell') },
  { id: 'batch', label: 'Batch', extensions: ['bat', 'cmd'], load: from(() => import('./languages/batch'), 'batch') },
  { id: 'perl', label: 'Perl', extensions: ['pl', 'pm'], load: from(() => import('@codemirror/legacy-modes/mode/perl'), 'perl') },
  { id: 'lua', label: 'Lua', extensions: ['lua'], load: from(() => import('@codemirror/legacy-modes/mode/lua'), 'lua') },
  { id: 'sql', label: 'SQL', extensions: ['sql'], load: async () => (await import('@codemirror/lang-sql')).sql() },
  { id: 'csharp', label: 'C#', extensions: ['cs'], load: from(() => import('@codemirror/legacy-modes/mode/clike'), 'csharp') },
  { id: 'java', label: 'Java', extensions: ['java'], names: ['jenkinsfile'], load: async () => (await import('@codemirror/lang-java')).java() },
  { id: 'go', label: 'Go', extensions: ['go'], load: async () => (await import('@codemirror/lang-go')).go() },
  { id: 'rust', label: 'Rust', extensions: ['rs'], load: async () => (await import('@codemirror/lang-rust')).rust() },
  { id: 'markdown', label: 'Markdown', extensions: ['md'], load: async () => (await import('@codemirror/lang-markdown')).markdown() },
  { id: 'html', label: 'HTML', extensions: ['html', 'htm'], load: async () => (await import('@codemirror/lang-html')).html() },
  { id: 'css', label: 'CSS', extensions: ['css'], load: async () => (await import('@codemirror/lang-css')).css() },
  { id: 'javascript', label: 'JavaScript', extensions: ['js', 'jsx'], load: async () => (await import('@codemirror/lang-javascript')).javascript({ jsx: true }) },
  { id: 'typescript', label: 'TypeScript', extensions: ['ts', 'tsx'], load: async () => (await import('@codemirror/lang-javascript')).javascript({ jsx: true, typescript: true }) },
  { id: 'matlab', label: 'MATLAB', extensions: ['m'], load: from(() => import('@codemirror/legacy-modes/mode/octave'), 'octave') },
  { id: 'cmake', label: 'CMake', extensions: ['cmake'], names: ['cmakelists.txt'], load: from(() => import('@codemirror/legacy-modes/mode/cmake'), 'cmake') },
  { id: 'makefile', label: 'Makefile', extensions: ['mk', 'mak'], names: ['makefile', 'gnumakefile'], load: from(() => import('./languages/makefile'), 'makefile') },
  { id: 'm4', label: 'm4', extensions: ['m4', 'ac'], load: from(() => import('./languages/m4'), 'm4') },
  { id: 'asap2', label: 'A2L (ASAP2)', extensions: ['a2l', 'aml'], load: from(() => import('./languages/asap2'), 'asap2') },
  { id: 'dbc', label: 'DBC', extensions: ['dbc'], load: from(() => import('./languages/dbc'), 'dbc') },
  { id: 'ldf', label: 'LDF', extensions: ['ldf'], load: from(() => import('./languages/ldf'), 'ldf') },
  { id: 'capl', label: 'CAPL', extensions: ['can', 'cin'], load: from(() => import('./languages/capl'), 'capl') },
  { id: 'oil', label: 'OSEK OIL', extensions: ['oil'], load: from(() => import('./languages/oil'), 'oil') },
  { id: 'linker', label: 'Linker script', extensions: ['ld', 'lds', 'lsl', 'lcf', 'icf', 'x'], load: from(() => import('./languages/linker'), 'linker') },
  { id: 'mapfile', label: 'Map file', extensions: ['map', 'lst'], load: from(() => import('./languages/mapfile'), 'mapfile') },
  { id: 'srec', label: 'S-record', extensions: ['s19', 's28', 's37', 'srec', 'mot'], load: from(() => import('./languages/srec'), 'srec') },
  { id: 'ihex', label: 'Intel HEX', extensions: ['hex', 'ihex'], load: from(() => import('./languages/ihex'), 'ihex') },
  { id: 'tlc', label: 'TLC', extensions: ['tlc'], load: from(() => import('./languages/tlc'), 'tlc') },
  { id: 'asm', label: 'Assembler', extensions: ['s', 'asm', 'inc'], load: from(() => import('./languages/asm'), 'asm') },
];

const FLAVOURS = ['arxml', 'epc', 'xdm', 'cdd', 'odx', 'pdx', 'cdfx', 'fibex', 'vsysvar', 'svd'];
const SNIFFED = [['bash', 'shell'], ['sh', 'shell'], ['zsh', 'shell'], ['python', 'python'], ['perl', 'perl']] as const;

export function findLanguage(id: string): LanguageEntry | undefined {
  return LANGUAGES.find(entry => entry.id === id);
}

export function extensionOf(path: string): string {
  const name = fileName(path);
  const dot = name.lastIndexOf('.');
  return dot < 0 ? '' : name.slice(dot + 1).toLowerCase();
}

const fileName = (path: string) => path.split(/[\\/]/).at(-1) ?? '';

function sniff(text: string): string | undefined {
  const lines = text.replace(/^﻿/, '').split(/\r?\n/, 8);
  const first = lines[0]?.trimStart() ?? '';
  if (first.startsWith('<?xml')) return 'xml';
  if (first.startsWith('#!')) return SNIFFED.find(([name]) => new RegExp(`\\b${name}[\\d.]*\\b`).test(first))?.[1];
  return lines.some(line => /^dnl\b/.test(line) || line.includes('divert(')) ? 'm4' : undefined;
}

export function resolveLanguage(path: string, userMap: LanguageMap = {}, text = ''): string {
  const extension = extensionOf(path), name = fileName(path).toLowerCase();
  const mapped = extension ? userMap[extension] : undefined;
  if (mapped && (mapped === PLAIN_TEXT || findLanguage(mapped))) return mapped;
  const known = LANGUAGES.find(entry => entry.names?.includes(name)) ?? (extension ? LANGUAGES.find(entry => entry.extensions.includes(extension)) : undefined);
  return known?.id ?? sniff(text) ?? PLAIN_TEXT;
}

export function languageId(path: string): string {
  return resolveLanguage(path);
}

export function languageLabel(path: string, id: string): string {
  const label = id === PLAIN_TEXT ? PLAIN_LABEL : findLanguage(id)?.label ?? PLAIN_LABEL;
  const extension = extensionOf(path);
  return id === 'xml' && FLAVOURS.includes(extension) ? `${extension.toUpperCase()} (${label})` : label;
}

export function normalizeExtension(value: string): string {
  const extension = value.trim().replace(/^\.+/, '').toLowerCase();
  return /^[a-z0-9_+-]{1,16}$/.test(extension) ? extension : '';
}

export function cleanLanguageMap(value: unknown): Record<string, string> {
  const clean: Record<string, string> = {};
  if (!value || typeof value !== 'object' || Array.isArray(value)) return clean;
  for (const [key, id] of Object.entries(value)) {
    const extension = normalizeExtension(key);
    if (extension && typeof id === 'string' && (id === PLAIN_TEXT || findLanguage(id))) clean[extension] = id;
  }
  return clean;
}
