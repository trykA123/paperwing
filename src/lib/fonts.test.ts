const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import { CODE_FONTS, UI_FONTS, fontCss } from './fonts';

describe('font choices', () => {
  test('retired ids fall back to Geist and Geist Mono', () => {
    for (const id of ['system', 'inter', 'plex', 'jetbrains', 'fira', 'source', 'unknown']) expect(fontCss(UI_FONTS, id)).toContain('Geist Variable');
    for (const id of ['cascadia', 'fira', 'plexmono', 'source', 'jetbrains', 'inter', 'system', 'plex', 'unknown']) expect(fontCss(CODE_FONTS, id)).toContain('Geist Mono Variable');
  });

  test('current ids resolve to their own stacks', () => {
    expect(fontCss(UI_FONTS, 'geist')).toContain('Geist Variable');
    expect(fontCss(UI_FONTS, 'native')).toContain('Segoe UI');
    expect(fontCss(CODE_FONTS, 'geist-mono')).toContain('Geist Mono Variable');
    expect(fontCss(CODE_FONTS, 'native')).toContain('Cascadia Code');
  });
});
