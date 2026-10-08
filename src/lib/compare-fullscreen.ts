import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { FullScreen } from './fullscreen.svelte';
import { app } from './state.svelte';

export const compareFullscreen = new FullScreen({
  window: () => (isTauri() ? getCurrentWindow() : null),
  activate: id => { if (app.tabs.some(tab => tab.id === id)) app.activateTab(id); else app.openView({ kind: 'repos' }); },
  fail: reason => app.toast(`Could not change full screen: ${reason}`, 'error'),
});
