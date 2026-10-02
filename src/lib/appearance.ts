import { getCurrentWindow } from '@tauri-apps/api/window';
import type { Theme } from './api';

export type FontChoice = { id: string; label: string; css: string };

export const UI_FONTS: FontChoice[] = [
  { id: 'system', label: 'Segoe UI (system)', css: '"Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif' },
  { id: 'inter', label: 'Inter', css: '"Inter Variable", Inter, system-ui, sans-serif' },
  { id: 'plex', label: 'IBM Plex Sans', css: '"IBM Plex Sans", system-ui, sans-serif' },
  { id: 'jetbrains', label: 'JetBrains Mono', css: '"JetBrains Mono Variable", "JetBrains Mono", monospace' },
  { id: 'fira', label: 'Fira Code', css: '"Fira Code Variable", "Fira Code", monospace' },
];

export const CODE_FONTS: FontChoice[] = [
  { id: 'cascadia', label: 'Cascadia Code (system)', css: '"Cascadia Code", Consolas, monospace' },
  { id: 'jetbrains', label: 'JetBrains Mono', css: '"JetBrains Mono Variable", "JetBrains Mono", monospace' },
  { id: 'fira', label: 'Fira Code', css: '"Fira Code Variable", "Fira Code", monospace' },
  { id: 'plexmono', label: 'IBM Plex Mono', css: '"IBM Plex Mono", monospace' },
  { id: 'source', label: 'Source Code Pro', css: '"Source Code Pro Variable", "Source Code Pro", monospace' },
];

const dark = window.matchMedia('(prefers-color-scheme: dark)');
const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)');

/** Duration for JS-driven transitions; 0 when the user prefers reduced motion. */
export const motionMs = (ms: number) => (reduceMotion.matches ? 0 : ms);
const fontCss = (list: FontChoice[], id: string) => (list.find(f => f.id === id) ?? list[0]).css;

export function resolvedTheme(theme: Theme): 'light' | 'dark' {
  return theme === 'system' ? (dark.matches ? 'dark' : 'light') : theme;
}

export function applyAppearance(theme: Theme, uiFont: string, codeFont: string) {
  const root = document.documentElement;
  root.dataset.theme = resolvedTheme(theme);
  root.style.setProperty('--font', fontCss(UI_FONTS, uiFont));
  root.style.setProperty('--mono', fontCss(CODE_FONTS, codeFont));
  // Title bar follows the choice too; null hands it back to Windows.
  getCurrentWindow().setTheme(theme === 'system' ? null : theme).catch(() => {});
}

/** Calls `cb` whenever Windows switches between light and dark. */
export function onSystemThemeChange(cb: () => void) {
  dark.addEventListener('change', cb);
  return () => dark.removeEventListener('change', cb);
}
