import { expect, test } from 'bun:test';
import base from '../../src-tauri/tauri.conf.json';
import windows from '../../src-tauri/tauri.windows.conf.json';

test('the Windows main window differs from the base only in decorations and shadow', () => {
  expect(windows.app.windows).toHaveLength(1);
  const { decorations, shadow, ...rest } = windows.app.windows[0];
  expect(decorations).toBe(false);
  expect(shadow).toBe(true);
  expect(rest).toEqual({ label: 'main', ...base.app.windows[0] });
});
