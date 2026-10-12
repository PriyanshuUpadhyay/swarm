import babel from "@rolldown/plugin-babel";
import react, { reactCompilerPreset } from "@vitejs/plugin-react";
import type { Plugin } from "vite";
import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";

// Vite dev serves CSS and the React refresh preamble inline, which the page CSP forbids.
// The CSP stays in the built pages, which are what `swarm serve` ships.
const dropCspInDev: Plugin = {
  name: "drop-csp-in-dev",
  apply: "serve",
  transformIndexHtml: (html) => html.replace(/<meta http-equiv="Content-Security-Policy"[^>]*>\s*/, ""),
};

const serve = process.env.SWARM_SERVE_URL;

export default defineConfig({
  plugins: [
    dropCspInDev,
    react({ include: /\/src\/react\/.*\.tsx?$/ }),
    babel({ include: [/\/src\/react\/.*\.tsx?$/], presets: [reactCompilerPreset()] }),
    // virtua ships Solid JSX source under the "solid" export condition; compile it as Solid too.
    solid({ include: [/\/src\/solid\/.*\.tsx?$/, /\/virtua\/lib\/solid\/.*\.jsx$/] }),
  ],
  build: {
    rollupOptions: { input: { index: "index.html", solid: "solid.html" } },
  },
  server: {
    // Keep the browser same-origin; the proxy rewrites Host to the serve port.
    proxy: serve ? { "/api": { target: new URL(serve).origin, changeOrigin: true } } : undefined,
  },
  // vite-plugin-solid asks for jsdom; the core tests need no DOM.
  test: { include: ["src/**/*.test.ts"], environment: "node" },
});
