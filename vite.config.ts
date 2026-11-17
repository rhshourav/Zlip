import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath } from 'node:url';

// Two pages share one build: the overlay (index.html) and the queue HUD (hud.html).
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    // WebView2 is evergreen Chromium: no legacy transpiling needed.
    target: 'chrome110',
    rollupOptions: {
      input: {
        main: fileURLToPath(new URL('./index.html', import.meta.url)),
        hud: fileURLToPath(new URL('./hud.html', import.meta.url))
      }
    }
  },
  test: { environment: 'node', include: ['src/**/*.test.ts'] }
});
