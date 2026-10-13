import babel from "@rolldown/plugin-babel";
import tailwindcss from "@tailwindcss/vite";
import react, { reactCompilerPreset } from "@vitejs/plugin-react";
import type { Plugin } from "vite";
import { defineConfig } from "vitest/config";

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
    tailwindcss(),
    react({ include: /\/src\/(react|components)\/.*\.tsx?$/ }),
    babel({ include: [/\/src\/(react|components)\/.*\.tsx?$/], presets: [reactCompilerPreset()] }),
  ],
  server: {
    // Keep the browser same-origin; the proxy rewrites Host to the serve port.
    proxy: serve ? { "/api": { target: new URL(serve).origin, changeOrigin: true } } : undefined,
  },
  test: { include: ["src/**/*.test.ts"], environment: "node" },
});
