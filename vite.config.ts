import { svelte } from '@sveltejs/vite-plugin-svelte';
import { existsSync, realpathSync } from 'node:fs';
import { defineConfig } from 'vite';

const modules = existsSync('node_modules') ? realpathSync('node_modules') : 'node_modules';

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, fs: { allow: ['.', modules] } },
  envPrefix: ['VITE_', 'TAURI_ENV_*'],
  build: { target: 'chrome110', outDir: 'dist' },
});
