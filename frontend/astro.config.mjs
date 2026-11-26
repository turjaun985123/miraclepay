// MiraclePay v0.0.1 — Astro 7 frontend configuration.
//
// Static output: the site is built to plain HTML/CSS/JS and can be
// served by any static host or by the Rust backend itself.
// All data is fetched client-side from the Rust API — the API base
// URL comes from the PUBLIC_API_URL environment variable and
// defaults to http://localhost:8080 when it is not set.
// No SSR adapter is needed for v0.0.1.
import { defineConfig } from 'astro/config';

export default defineConfig({
  output: 'static',
  server: {
    port: 4321,
  },
  vite: {
    define: {},
  },
});
