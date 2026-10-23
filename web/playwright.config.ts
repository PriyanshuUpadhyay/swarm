import { defineConfig, devices } from "@playwright/test";

// Needs `pnpm build`, `cargo build --features serve` and the zig transcript tool first.
export default defineConfig({
  testDir: "e2e",
  globalSetup: "./e2e/serve.ts",
  workers: 1,
  // The app ships only in WebKit (the Swarm app and Safari).
  projects: [{ name: "webkit", use: { ...devices["Desktop Safari"] } }],
});
