import { defineConfig, devices } from "@playwright/test";

// Needs `pnpm build`, `cargo build --features serve` and the zig transcript tool first.
export default defineConfig({
  testDir: "e2e",
  globalSetup: "./e2e/serve.ts",
  workers: 1,
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "webkit", use: { ...devices["Desktop Safari"] } },
  ],
});
