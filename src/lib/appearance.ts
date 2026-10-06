import { getCurrentWindow } from '@tauri-apps/api/window';
import type { Theme } from './api';
import { CODE_FONTS, UI_FONTS, fontCss } from './fonts';

export { CODE_FONTS, UI_FONTS, type FontChoice } from './fonts';

const dark = window.matchMedia('(prefers-color-scheme: dark)');
const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)');

/** Duration for JS-driven transitions; 0 when the user prefers reduced motion. */
export const motionMs = (ms: number) => (reduceMotion.matches ? 0 : ms);

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
