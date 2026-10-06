import { getCurrentWindow } from '@tauri-apps/api/window';
import type { Theme } from './api';

export type FontChoice = { id: string; label: string; css: string };

export const UI_FONTS: FontChoice[] = [
  { id: 'geist', label: 'Geist', css: '"Geist Variable", system-ui, sans-serif' },
  { id: 'native', label: 'System UI', css: '"Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif' },
];

export const CODE_FONTS: FontChoice[] = [
  { id: 'geist-mono', label: 'Geist Mono', css: '"Geist Mono Variable", ui-monospace, monospace' },
  { id: 'native', label: 'System monospace', css: '"Cascadia Code", Consolas, ui-monospace, monospace' },
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
