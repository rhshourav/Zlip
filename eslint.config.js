import js from '@eslint/js';
import globals from 'globals';
import svelte from 'eslint-plugin-svelte';
import tseslint from 'typescript-eslint';
import { defineConfig, globalIgnores } from 'eslint/config';

export default defineConfig(
  globalIgnores(['dist', 'node_modules', 'target', 'src-tauri', 'crates']),
  js.configs.recommended,
  tseslint.configs.recommended,
  svelte.configs.recommended,
  {
    languageOptions: { globals: { ...globals.browser } },
    rules: {
      // `{@html}` is used only for HTML that zlip-highlight escapes in Rust.
      // Each use carries an eslint-disable comment naming that reason.
      'svelte/no-at-html-tags': 'error',
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_', varsIgnorePattern: '^_' }]
    }
  },
  {
    files: ['**/*.svelte', '**/*.svelte.ts'],
    languageOptions: { parserOptions: { parser: tseslint.parser } }
  },
  { files: ['*.config.js', 'vite.config.ts'], languageOptions: { globals: { ...globals.node } } },
  // Release tooling (scripts/) runs in Node, not in the webview.
  { files: ['scripts/**/*.mjs'], languageOptions: { globals: { ...globals.node } } }
);
