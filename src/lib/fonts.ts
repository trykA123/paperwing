export type FontChoice = { id: string; label: string; css: string };

export const UI_FONTS: FontChoice[] = [
  { id: 'geist', label: 'Geist', css: '"Geist Variable", system-ui, sans-serif' },
  { id: 'native', label: 'System UI', css: '"Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif' },
];

export const CODE_FONTS: FontChoice[] = [
  { id: 'geist-mono', label: 'Geist Mono', css: '"Geist Mono Variable", ui-monospace, monospace' },
  { id: 'native', label: 'System monospace', css: '"Cascadia Code", Consolas, ui-monospace, monospace' },
];

export const fontCss = (list: FontChoice[], id: string) => (list.find(f => f.id === id) ?? list[0]).css;
