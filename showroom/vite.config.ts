import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Pages serves the site from https://marvinbaudach.github.io/reprise/, so every
// asset URL needs that prefix. Getting this wrong produces a page that loads
// locally and 404s in production — the one failure mode a preview never shows.
export default defineConfig({
  base: '/reprise/',
  plugins: [react()],
  build: {
    target: 'es2022',
    cssCodeSplit: false,
    assetsInlineLimit: 2048,
  },
});
