import { defineConfig, devices } from "@playwright/test";

// Needs `pnpm build`, `cargo build --features serve` and the zig transcript tool first.
export default defineConfig({
  testDir: "e2e",
  globalSetup: "./e2e/serve.ts",
  workers: 1,
  // Every wait is on app state, so these limits matter only when a case fails. A loaded Mac makes
  // one server read take seconds, so the 5 s and 30 s defaults fail cases that are correct.
  timeout: 120_000,
  expect: { timeout: 20_000 },
  // The app ships only in WebKit (the Swarm app and Safari).
  projects: [{ name: "webkit", use: { ...devices["Desktop Safari"] } }],
});
