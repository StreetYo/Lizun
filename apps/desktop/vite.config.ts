import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

const page = (file: string) => fileURLToPath(new URL(file, import.meta.url));

// Tauri loads the dev server from a fixed port (see src-tauri/tauri.conf.json).
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: 'es2022',
    rollupOptions: {
      input: {
        island: page('./index.html'),
        settings: page('./settings.html'),
      },
    },
  },
});
