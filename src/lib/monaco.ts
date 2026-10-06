import * as monaco from 'monaco-editor';
import EditorWorker from 'monaco-editor/editor/editor.worker?worker';
import CssWorker from 'monaco-editor/language/css/css.worker?worker';
import HtmlWorker from 'monaco-editor/language/html/html.worker?worker';
import JsonWorker from 'monaco-editor/language/json/json.worker?worker';
import TypeScriptWorker from 'monaco-editor/language/typescript/ts.worker?worker';

const environment = globalThis as typeof globalThis & { MonacoEnvironment?: { getWorker: (id: string, label: string) => Worker } };
environment.MonacoEnvironment = { getWorker: (_id, label) => {
  if (label === 'typescript' || label === 'javascript') return new TypeScriptWorker();
  if (label === 'json') return new JsonWorker();
  if (label === 'css' || label === 'scss' || label === 'less') return new CssWorker();
  if (label === 'html' || label === 'handlebars' || label === 'razor') return new HtmlWorker();
  return new EditorWorker();
} };
export { monaco };

export function applyEditorTheme(): string {
  const dark = document.documentElement.dataset.theme === 'dark';
  const style = getComputedStyle(document.documentElement);
  const canvas = document.createElement('canvas'); canvas.width = canvas.height = 1;
  const context = canvas.getContext('2d')!;
  const color = (token: string) => {
    context.clearRect(0, 0, 1, 1);
    context.fillStyle = style.getPropertyValue(token).trim(); context.fillRect(0, 0, 1, 1);
    return '#' + Array.from(context.getImageData(0, 0, 1, 1).data).slice(0, 3).map(channel => channel.toString(16).padStart(2, '0')).join('');
  };
  const name = dark ? 'skein-dark' : 'skein-light';
  monaco.editor.defineTheme(name, { base: dark ? 'vs-dark' : 'vs', inherit: true, rules: [], colors: {
    'editor.background': color('--editor'), 'editor.foreground': color('--text'),
    'editorGutter.background': color('--editor'), 'editorLineNumber.foreground': color('--dim'),
    'editorLineNumber.activeForeground': color('--text'), 'editorCursor.foreground': color('--acc'),
    'editor.selectionBackground': color('--accs'), 'editor.inactiveSelectionBackground': color('--soft'),
    'editorOverviewRuler.border': color('--line'), 'editorWidget.background': color('--panel'),
    'editorWidget.border': color('--line'), 'focusBorder': color('--focus'),
  } });
  monaco.editor.setTheme(name);
  return name;
}

export function language(path: string): string {
  const extension = path.split('.').at(-1)?.toLowerCase() ?? '';
  return ({ c: 'cpp', h: 'cpp', cpp: 'cpp', hpp: 'cpp', rs: 'rust', py: 'python', js: 'javascript', jsx: 'javascript',
    ts: 'typescript', tsx: 'typescript', xml: 'xml', arxml: 'xml', yml: 'yaml', yaml: 'yaml' } as Record<string, string>)[extension] ?? 'plaintext';
}